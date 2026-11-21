//! Boot-time wiring between the pure state machine and the I/O shell, plus
//! the in-process runtime API ([`HotUpdate`]).
//!
//! Ordering guarantee (the anti-brick invariant): [`initialize`] runs inside
//! the plugin's setup hook, which tauri executes during `Builder::build`
//! (tauri 2.10.2 `app.rs:2289`; re-verified on 2.11.5 `app.rs:2440`) —
//! strictly before any window or webview is created, and therefore before
//! the assets provider serves a single byte. The staged→booting promotion
//! is persisted inside [`initialize`], so by the time serving starts the rollback marker is
//! already on disk. If persisting fails, the newly armed bundle is *not*
//! served (a trial boot without a persisted marker could evade rollback).

use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use semver::Version;
use serde::Serialize;

use crate::machine::{self, AckOutcome, Active};
use crate::store::Store;
use crate::{Error, Result};

/// Activation shared by the asset provider and native startup updater.
#[derive(Default)]
pub(crate) struct Shared {
    activation: RwLock<Option<Arc<Activation>>>,
}

/// Snapshot of one activation. Existing requests retain their snapshot.
pub(crate) struct Activation {
    store: Arc<Store>,
    /// The source currently served; replaced only before navigating into the game.
    active: Active,
    /// Bundle dir for [`Active::Ota`], resolved once.
    active_dir: Option<PathBuf>,
    /// Version of the active bundle (OTA) or of the embedded assets.
    active_version: Version,
    embedded_version: Version,
    /// Serializes update pipeline runs ([`crate::update`]): concurrent
    /// check/download calls must not race seq allocation or double-download.
    update_lock: Arc<tokio::sync::Mutex<()>>,
}

impl Activation {
    pub(crate) fn store(&self) -> &Store {
        &self.store
    }

    /// The shell version the compatibility gate checks `minShellVersion`
    /// against. `version:bump` keeps the embedded frontend in lockstep with
    /// the app version, so this is also the embedded bundle version.
    pub(crate) fn embedded_version(&self) -> &Version {
        &self.embedded_version
    }

    pub(crate) fn update_lock(&self) -> &tokio::sync::Mutex<()> {
        &self.update_lock
    }
}

impl Shared {
    /// Directory the provider should serve from, or `None` for embedded.
    /// `None` before activation: the fail-safe default is embedded.
    pub(crate) fn active_dir(&self) -> Option<PathBuf> {
        self.activation.read().unwrap().as_ref()?.active_dir.clone()
    }

    pub(crate) fn is_activated(&self) -> bool {
        self.activation.read().unwrap().is_some()
    }

    pub(crate) fn activation(&self) -> Result<Arc<Activation>> {
        self.activation.read().unwrap().clone().ok_or(Error::NotActive)
    }
}

/// Run boot resolution against `root` (the `hot-update/` dir) and activate
/// serving. Called from the plugin setup hook; must never panic or fail the
/// app — every failure path degrades to serving embedded assets.
pub(crate) fn initialize(shared: &Shared, root: PathBuf, embedded_version: Version) {
    let store = Arc::new(Store::new(root));
    let state = store.load_state();
    let present = store.present_seqs();
    let outcome = machine::resolve_boot(state, &embedded_version, &present);

    if let Some(seq) = outcome.rolled_back {
        log::warn!(
            "hot-update: bundle seq {seq} was armed last boot but never acked; \
             rolled back and blacklisted its archive hash"
        );
    }

    // Persist BEFORE serving. On failure, fall back to what the previous
    // persisted state already supports: the committed bundle survived boot
    // validation and its pointer is already durable, but a *newly armed*
    // bundle must not run without its rollback marker on disk.
    let active = match store.save_state(&outcome.state) {
        Ok(()) => {
            store.apply_effects(&outcome.effects);
            store.sweep_foreign_entries();
            outcome.active
        }
        Err(e) => {
            log::error!(
                "hot-update: failed to persist state.json ({e}); \
                 serving committed/embedded without arming the staged bundle"
            );
            match outcome.state.committed {
                Some(seq) => Active::Ota(seq),
                None => Active::Embedded,
            }
        }
    };

    let (active_dir, active_version) = match active {
        Active::Ota(seq) => {
            // resolve_boot only arms/keeps seqs it has metadata for.
            let version = outcome
                .state
                .versions
                .get(&seq)
                .map(|meta| meta.version.clone())
                .unwrap_or_else(|| embedded_version.clone());
            (Some(store.bundle_dir(seq)), version)
        }
        Active::Embedded => (None, embedded_version.clone()),
    };

    match active {
        Active::Ota(seq) => log::info!(
            "hot-update: serving OTA bundle seq {seq} (v{active_version}) from {}",
            store.bundle_dir(seq).display()
        ),
        Active::Embedded => {
            log::info!("hot-update: serving embedded assets (v{embedded_version})")
        }
    }

    let activation = Activation {
        store,
        active,
        active_dir,
        active_version,
        embedded_version,
        update_lock: Arc::new(tokio::sync::Mutex::new(())),
    };
    let mut slot = shared.activation.write().unwrap();
    if slot.is_none() { *slot = Some(Arc::new(activation)); }
}

/// Where the currently served frontend comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BundleSource {
    Embedded,
    Ota,
}

/// Snapshot of what this process is serving.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentBundle {
    pub source: BundleSource,
    /// Bundle seq when `source == Ota`.
    pub seq: Option<u64>,
    pub version: Version,
}

/// Runtime API, managed in the app state by [`crate::init`]. IPC commands
/// (WP4) will be thin wrappers over these methods.
pub struct HotUpdate {
    pub(crate) shared: Arc<Shared>,
}

impl HotUpdate {
    /// Activate a staged bundle before navigating from the embedded startup page.
    /// No game assets may be in flight. A trial must acknowledge before another activation.
    pub async fn activate_staged(&self) -> Result<CurrentBundle> {
        let previous = self.shared.activation()?;
        let _serialized = previous.update_lock().lock().await;
        let store = &previous.store;
        let state = store.load_state();
        if state.staged.is_none() { return self.current_bundle(); }
        if state.booting.is_some() {
            return Err(Error::Config("cannot replace an unacknowledged trial bundle".into()));
        }
        let outcome = machine::resolve_boot(state, &previous.embedded_version, &store.present_seqs());
        // Persist the rollback marker before the asset provider can serve the trial.
        store.save_state(&outcome.state)?;
        let (active_dir, active_version) = match outcome.active {
            Active::Ota(seq) => (Some(store.bundle_dir(seq)), outcome.state.versions[&seq].version.clone()),
            Active::Embedded => (None, previous.embedded_version.clone()),
        };
        let activation = Activation {
            store: Arc::clone(store),
            active: outcome.active, active_dir, active_version,
            embedded_version: previous.embedded_version.clone(),
            update_lock: Arc::clone(&previous.update_lock),
        };
        *self.shared.activation.write().unwrap() = Some(Arc::new(activation));
        store.apply_effects(&outcome.effects);
        Ok(self.current_bundle()?)
    }
    /// Commit the bundle this process booted (`notifyAppReady`).
    ///
    /// Commits the in-memory booted seq captured at boot resolution — never
    /// a value re-read from disk — so a download that finished mid-session
    /// stays staged for its own trial boot. Idempotent.
    pub fn notify_app_ready(&self) -> Result<AckOutcome> {
        let activation = self.shared.activation()?;
        let (outcome, effects) = activation.store.update(|state| {
            let (state, outcome, effects) = machine::ack(state, activation.active);
            (state, (outcome, effects))
        })?;
        activation.store.apply_effects(&effects);
        match outcome {
            AckOutcome::Committed(seq) => {
                log::info!("hot-update: bundle seq {seq} committed as last-good")
            }
            AckOutcome::Stale(seq) => {
                log::warn!("hot-update: ack for seq {seq} no longer matches on-disk state; ignored")
            }
            AckOutcome::AlreadyCommitted(_) | AckOutcome::EmbeddedNoop => {}
        }
        Ok(outcome)
    }

    /// What is being served right now.
    pub fn current_bundle(&self) -> Result<CurrentBundle> {
        let activation = self.shared.activation()?;
        Ok(match activation.active {
            Active::Ota(seq) => CurrentBundle {
                source: BundleSource::Ota,
                seq: Some(seq),
                version: activation.active_version.clone(),
            },
            Active::Embedded => CurrentBundle {
                source: BundleSource::Embedded,
                seq: None,
                version: activation.embedded_version.clone(),
            },
        })
    }

    /// Debug/support escape hatch: wipe all OTA state and bundles, restart
    /// the watermark from the embedded version. The currently served bundle
    /// dir (if any) is left in place until the next boot's GC so the running
    /// webview is not torn; serving reverts to embedded on the next launch.
    pub fn reset(&self) -> Result<()> {
        let activation = self.shared.activation()?;
        let effects = activation.store.update(|state| {
            machine::reset(state, &activation.embedded_version, activation.active)
        })?;
        activation.store.apply_effects(&effects);
        activation.store.sweep_foreign_entries();
        log::info!("hot-update: state reset; embedded serving resumes next launch");
        Ok(())
    }
}

#[cfg(test)]
mod tests;
