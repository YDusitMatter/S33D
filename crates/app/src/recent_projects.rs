//! Recently used editable projects live in UI preferences, never in a project.
use std::path::{Path, PathBuf};

pub const LIMIT: usize = 10;

fn absolute_path(path: &Path) -> Option<PathBuf> {
    if path.as_os_str().is_empty() {
        return None;
    }
    // Preferences may contain offline network drives. Normalize the spelling
    // without probing the filesystem or waiting for an unavailable server.
    let path = std::path::absolute(path).ok()?;
    #[cfg(windows)]
    {
        // Keep the ordinary Windows path readable in menus and preferences.
        let text = path.to_string_lossy();
        if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
            return Some(PathBuf::from(format!(r"\\{rest}")));
        }
        if let Some(rest) = text.strip_prefix(r"\\?\") {
            return Some(PathBuf::from(rest));
        }
    }
    Some(path)
}

fn same_path(a: &Path, b: &Path) -> bool {
    #[cfg(windows)]
    {
        a.as_os_str().to_string_lossy().to_lowercase()
            == b.as_os_str().to_string_lossy().to_lowercase()
    }
    #[cfg(not(windows))]
    {
        a == b
    }
}

pub fn normalize(paths: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut result: Vec<PathBuf> = Vec::new();
    for path in paths {
        if let Some(path) = absolute_path(&path) {
            if !result.iter().any(|p| same_path(p, &path)) {
                result.push(path);
            }
        }
        if result.len() == LIMIT {
            break;
        }
    }
    result
}

pub fn remember(paths: &mut Vec<PathBuf>, path: &Path) {
    *paths = normalize(std::iter::once(path.to_path_buf()).chain(std::mem::take(paths)));
}

pub fn name(path: &Path) -> String {
    path.file_stem()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recency_keeps_ten_unique_absolute_paths_and_survives_preferences_restart() {
        let mut settings = crate::app::AppSettings::default();
        for i in 0..12 {
            remember(
                &mut settings.recent_projects,
                &PathBuf::from(format!("项目 {i}.s33d")),
            );
        }
        assert_eq!(settings.recent_projects.len(), 10);
        assert_eq!(name(&settings.recent_projects[0]), "项目 11");
        let older = settings.recent_projects[7].clone();
        remember(&mut settings.recent_projects, &older);
        assert_eq!(settings.recent_projects[0], older);
        assert_eq!(settings.recent_projects.len(), 10);
        assert!(settings.recent_projects.iter().all(|p| p.is_absolute()));
        let reopened: crate::app::AppSettings =
            serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
        assert_eq!(reopened.recent_projects, settings.recent_projects);
        assert_eq!(
            serde_json::from_str::<crate::app::AppSettings>("{}")
                .unwrap()
                .recent_projects,
            Vec::<PathBuf>::new()
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_case_aliases_do_not_consume_multiple_recent_slots() {
        let mut paths = Vec::new();
        remember(&mut paths, Path::new(r"D:\example\Model.s33d"));
        remember(&mut paths, Path::new(r"d:\EXAMPLE\model.s33d"));
        assert_eq!(paths.len(), 1);
    }
}
