//! What happens to each new clip: filter, dedupe, save.

use std::path::Path;

use crate::images::{self, RawImage, MAX_IMAGE_BYTES};
use crate::settings::Settings;
use crate::store::{NewItem, Store};

#[derive(Debug, Clone, PartialEq)]
pub enum ClipContent {
    Text(String),
    Image(RawImage),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Clip {
    pub content: ClipContent,
    pub secret: bool,
    pub source_app: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Added(i64),
    /// A repeat. The existing item moved to the top.
    Bumped(i64),
    Skipped(&'static str),
}

/// Our own clipboard writes come back through here too. Dedupe turns them
/// into a harmless bump.
pub fn capture(
    store: &Store,
    settings: &Settings,
    images_dir: &Path,
    clip: Clip,
    now_ms: i64,
) -> crate::Result<Outcome> {
    if clip.secret && settings.skip_secrets {
        return Ok(Outcome::Skipped("secret"));
    }
    if let Some(app) = &clip.source_app {
        if settings.ignored_apps.iter().any(|a| a.eq_ignore_ascii_case(app)) {
            return Ok(Outcome::Skipped("ignored app"));
        }
    }
    let source = clip.source_app.as_deref();

    match clip.content {
        ClipContent::Text(text) => {
            if text.trim().is_empty() {
                return Ok(Outcome::Skipped("blank text"));
            }
            let (id, is_new) = store.insert(&NewItem::text(&text, source), now_ms)?;
            Ok(if is_new { Outcome::Added(id) } else { Outcome::Bumped(id) })
        }
        ClipContent::Image(img) => {
            if !settings.capture_images {
                return Ok(Outcome::Skipped("images off"));
            }
            // Check for a repeat before writing any files.
            let hash = img.hash();
            if let Some(id) = store.bump_hash(&hash, now_ms)? {
                return Ok(Outcome::Bumped(id));
            }
            let Some((png, thumb)) = images::save(images_dir, &hash, &img, MAX_IMAGE_BYTES)? else {
                return Ok(Outcome::Skipped("image too large"));
            };
            let (id, _) = store.insert(&NewItem::image(&png, &thumb, hash, source), now_ms)?;
            Ok(Outcome::Added(id))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Kind;

    fn text(t: &str) -> Clip {
        Clip { content: ClipContent::Text(t.into()), secret: false, source_app: None }
    }

    fn image(seed: u8) -> Clip {
        let rgba = [seed, 0, 0, 255].repeat(16);
        Clip {
            content: ClipContent::Image(RawImage { width: 4, height: 4, rgba }),
            secret: false,
            source_app: Some("Paint".into()),
        }
    }

    struct Env {
        store: Store,
        dir: tempfile::TempDir,
        settings: Settings,
    }

    impl Env {
        fn new() -> Env {
            Env {
                store: Store::open_in_memory().unwrap(),
                dir: tempfile::tempdir().unwrap(),
                settings: Settings::default(),
            }
        }

        fn run(&self, clip: Clip, now: i64) -> Outcome {
            capture(&self.store, &self.settings, self.dir.path(), clip, now).unwrap()
        }

        fn files(&self) -> usize {
            std::fs::read_dir(self.dir.path()).unwrap().count()
        }
    }

    #[test]
    fn new_text_is_added() {
        let env = Env::new();
        let Outcome::Added(id) = env.run(text("hello"), 1) else { panic!("not added") };
        assert_eq!(env.store.get(id).unwrap().unwrap().text.as_deref(), Some("hello"));
    }

    #[test]
    fn repeat_text_is_bumped() {
        let env = Env::new();
        let Outcome::Added(id) = env.run(text("hello"), 1) else { panic!("not added") };
        assert_eq!(env.run(text("hello"), 2), Outcome::Bumped(id));
        assert_eq!(env.store.list(10).unwrap().len(), 1);
    }

    #[test]
    fn blank_text_is_skipped() {
        let env = Env::new();
        assert!(matches!(env.run(text(" \n\t"), 1), Outcome::Skipped(_)));
        assert!(env.store.list(10).unwrap().is_empty());
    }

    #[test]
    fn secret_is_saved_when_filter_is_off() {
        let env = Env::new();
        let clip = Clip { secret: true, ..text("hunter2") };
        assert!(matches!(env.run(clip, 1), Outcome::Added(_)));
    }

    #[test]
    fn secret_is_skipped_when_filter_is_on() {
        let mut env = Env::new();
        env.settings.skip_secrets = true;
        let clip = Clip { secret: true, ..text("hunter2") };
        assert!(matches!(env.run(clip, 1), Outcome::Skipped(_)));
        assert!(env.store.list(10).unwrap().is_empty());
    }

    #[test]
    fn ignored_app_is_skipped_ignoring_case() {
        let mut env = Env::new();
        env.settings.ignored_apps = vec!["keepassxc".into()];
        let clip = Clip { source_app: Some("KeePassXC".into()), ..text("pw") };
        assert!(matches!(env.run(clip, 1), Outcome::Skipped(_)));
        let other = Clip { source_app: Some("notepad".into()), ..text("pw") };
        assert!(matches!(env.run(other, 2), Outcome::Added(_)));
    }

    #[test]
    fn new_image_saves_files_and_row() {
        let env = Env::new();
        let Outcome::Added(id) = env.run(image(1), 1) else { panic!("not added") };
        let item = env.store.get(id).unwrap().unwrap();
        assert_eq!(item.kind, Kind::Image);
        assert_eq!(item.source_app.as_deref(), Some("Paint"));
        assert!(Path::new(item.image_path.as_ref().unwrap()).exists());
        assert!(Path::new(item.thumb_path.as_ref().unwrap()).exists());
        assert_eq!(env.files(), 2);
    }

    #[test]
    fn repeat_image_is_bumped_without_new_files() {
        let env = Env::new();
        let Outcome::Added(id) = env.run(image(1), 1) else { panic!("not added") };
        assert_eq!(env.run(image(1), 2), Outcome::Bumped(id));
        assert_eq!(env.files(), 2);
        assert!(matches!(env.run(image(2), 3), Outcome::Added(_)));
        assert_eq!(env.files(), 4);
    }

    #[test]
    fn images_are_skipped_when_capture_images_is_off() {
        let mut env = Env::new();
        env.settings.capture_images = false;
        assert!(matches!(env.run(image(1), 1), Outcome::Skipped(_)));
        assert_eq!(env.files(), 0);
    }
}
