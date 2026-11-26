//! In-process reservation for library mutations and structural previews.
//! Acquire before any module lock; never acquire recursively.
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
};

static ACTIVE: LazyLock<Mutex<HashSet<PathBuf>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

#[must_use]
pub(crate) struct Reservation {
    root: PathBuf,
}
impl Reservation {
    pub(crate) fn acquire(root: &Path) -> Result<Self, String> {
        let root = fs::canonicalize(root).map_err(|e| format!("Biblioteca inválida: {e}"))?;
        if !root.is_dir() {
            return Err("Biblioteca inválida.".into());
        }
        let mut active = ACTIVE
            .lock()
            .map_err(|_| "Coordenador de biblioteca indisponível.")?;
        if active
            .iter()
            .any(|p| p.starts_with(&root) || root.starts_with(p))
        {
            return Err(
                "Outra operação está em andamento nesta biblioteca. Tente novamente.".into(),
            );
        }
        active.insert(root.clone());
        drop(active);
        let reservation = Self { root };
        if crate::library_journal::inspect(reservation.root())?.is_some_and(|state| state.pending())
        {
            return Err("Há uma reorganização pendente de recuperação nesta biblioteca.".into());
        }
        Ok(reservation)
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }
}
impl Drop for Reservation {
    fn drop(&mut self) {
        // No filesystem work or module lock while holding the registry mutex.
        if let Ok(mut active) = ACTIVE.lock() {
            active.remove(&self.root);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reservations_exclude_same_and_nested_roots_but_allow_independent_libraries() {
        let base = std::env::temp_dir().join(format!(
            "acervo-reservation-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(base.join("A/Nested")).unwrap();
        fs::create_dir(base.join("AB")).unwrap();
        let guard = Reservation::acquire(&base.join("A")).unwrap();
        let a = base.join("A/Nested/..");
        let child = base.join("A/Nested");
        let parent = base.clone();
        std::thread::spawn(move || {
            assert!(Reservation::acquire(&a).is_err());
            assert!(Reservation::acquire(&child).is_err());
            assert!(Reservation::acquire(&parent).is_err());
        })
        .join()
        .unwrap();
        let independent = Reservation::acquire(&base.join("AB")).unwrap();
        drop(guard);
        let child = Reservation::acquire(&base.join("A/Nested")).unwrap();
        assert!(Reservation::acquire(&base.join("A")).is_err());
        drop(child);
        drop(independent);
        assert!(Reservation::acquire(&base).is_ok());
        fs::remove_dir_all(base).unwrap();
    }
}
