use crate::client::Client;
use std::{
    io,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

/// The server's view of the client directory. Every request takes a snapshot,
/// so a reindex can swap a freshly built one in without disturbing the
/// requests already in flight.
pub struct Shared {
    root: PathBuf,
    client: RwLock<Arc<Client>>,
}

impl Shared {
    pub fn open(root: &Path) -> io::Result<Shared> {
        Ok(Shared {
            root: root.to_path_buf(),
            client: RwLock::new(Arc::new(Client::open(root)?)),
        })
    }

    /// The index as it stands. The lock is only held long enough to bump a
    /// refcount, never for the length of a request.
    pub fn client(&self) -> Arc<Client> {
        Arc::clone(&self.client.read().unwrap())
    }

    /// Rebuild the index from the same root. The running one keeps serving
    /// until the new one is complete, and stays in place if the build fails.
    pub fn reload(&self) -> io::Result<()> {
        let fresh = Client::open(&self.root)?;
        *self.client.write().unwrap() = Arc::new(fresh);
        Ok(())
    }
}
