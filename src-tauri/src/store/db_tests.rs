use super::*;

fn texts(items: &[Item]) -> Vec<&str> {
    items.iter().map(|i| i.text.as_deref().unwrap_or("")).collect()
}

#[test]
fn hash_is_sha256_hex() {
    assert_eq!(
        hash_bytes(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn inserted_text_comes_back_with_its_fields() {
    let store = Store::open_in_memory().unwrap();
    let (id, is_new) = store.insert(&NewItem::text("hello", Some("notepad")), 1000).unwrap();
    assert!(is_new);

    let items = store.list(10).unwrap();
    assert_eq!(items.len(), 1);
    let item = &items[0];
    assert_eq!(item.id, id);
    assert_eq!(item.kind, Kind::Text);
    assert_eq!(item.text.as_deref(), Some("hello"));
    assert_eq!(item.hash, hash_bytes(b"hello"));
    assert_eq!(item.created_at, 1000);
    assert_eq!(item.last_used_at, 1000);
    assert!(!item.pinned);
    assert_eq!(item.source_app.as_deref(), Some("notepad"));
}

#[test]
fn list_is_newest_first_and_respects_limit() {
    let store = Store::open_in_memory().unwrap();
    store.insert(&NewItem::text("one", None), 1).unwrap();
    store.insert(&NewItem::text("two", None), 2).unwrap();
    store.insert(&NewItem::text("three", None), 3).unwrap();

    assert_eq!(texts(&store.list(10).unwrap()), ["three", "two", "one"]);
    assert_eq!(texts(&store.list(2).unwrap()), ["three", "two"]);
}

#[test]
fn repeat_copy_moves_existing_row_to_top() {
    let store = Store::open_in_memory().unwrap();
    let (first_id, _) = store.insert(&NewItem::text("same", None), 1).unwrap();
    store.insert(&NewItem::text("other", None), 2).unwrap();

    let (id, is_new) = store.insert(&NewItem::text("same", None), 3).unwrap();
    assert_eq!(id, first_id);
    assert!(!is_new);

    let items = store.list(10).unwrap();
    assert_eq!(texts(&items), ["same", "other"]);
    assert_eq!(items[0].created_at, 1, "repeat keeps the original created_at");
    assert_eq!(items[0].last_used_at, 3);
}

#[test]
fn data_survives_reopening_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.db");
    Store::open(&path).unwrap().insert(&NewItem::text("kept", None), 1).unwrap();

    let reopened = Store::open(&path).unwrap();
    assert_eq!(texts(&reopened.list(10).unwrap()), ["kept"]);
}

fn store_with(texts_in_order: &[&str]) -> Store {
    let store = Store::open_in_memory().unwrap();
    for (i, t) in texts_in_order.iter().enumerate() {
        store.insert(&NewItem::text(t, None), i as i64 + 1).unwrap();
    }
    store
}

#[test]
fn empty_search_lists_everything() {
    let store = store_with(&["one", "two"]);
    assert_eq!(texts(&store.search("", 10).unwrap()), ["two", "one"]);
    assert_eq!(texts(&store.search("   ", 10).unwrap()), ["two", "one"]);
}

#[test]
fn search_finds_mid_word_substring_ignoring_case() {
    let store = store_with(&["Unbelievable", "nothing here", "BELIEVE it"]);
    assert_eq!(texts(&store.search("lieV", 10).unwrap()), ["BELIEVE it", "Unbelievable"]);
}

#[test]
fn short_queries_still_find_substrings() {
    let store = store_with(&["xyz", "abc", "zab"]);
    assert_eq!(texts(&store.search("ab", 10).unwrap()), ["zab", "abc"]);
    assert_eq!(texts(&store.search("Y", 10).unwrap()), ["xyz"]);
}

#[test]
fn short_query_treats_like_wildcards_literally() {
    let store = store_with(&["abc", "100%", "a_c"]);
    assert_eq!(texts(&store.search("%", 10).unwrap()), ["100%"]);
    assert_eq!(texts(&store.search("_", 10).unwrap()), ["a_c"]);
}

#[test]
fn search_handles_fts_syntax_characters_as_plain_text() {
    let store = store_with(&["say \"hi\" OR bye", "c++ code", "plain"]);
    assert_eq!(texts(&store.search("\"hi\" OR", 10).unwrap()), ["say \"hi\" OR bye"]);
    assert_eq!(texts(&store.search("c++", 10).unwrap()), ["c++ code"]);
    assert!(store.search("NEAR(", 10).unwrap().is_empty());
}

#[test]
fn search_sees_dedupe_updates_and_respects_limit() {
    let store = store_with(&["apple pie", "apple tart", "apple jam"]);
    store.insert(&NewItem::text("apple pie", None), 10).unwrap();
    assert_eq!(texts(&store.search("apple", 2).unwrap()), ["apple pie", "apple jam"]);
}

#[test]
fn toggle_pin_flips_state_and_pinned_items_list_first() {
    let store = store_with(&["old item", "new item"]);
    let old_id = store.list(10).unwrap()[1].id;

    assert_eq!(store.toggle_pin(old_id).unwrap(), Some(true));
    let items = store.list(10).unwrap();
    assert_eq!(texts(&items), ["old item", "new item"]);
    assert!(items[0].pinned);
    assert_eq!(texts(&store.search("it", 10).unwrap()), ["old item", "new item"]);

    assert_eq!(store.toggle_pin(old_id).unwrap(), Some(false));
    assert_eq!(texts(&store.list(10).unwrap()), ["new item", "old item"]);
}

#[test]
fn toggle_pin_on_missing_id_returns_none() {
    let store = Store::open_in_memory().unwrap();
    assert_eq!(store.toggle_pin(42).unwrap(), None);
}

#[test]
fn delete_removes_row_from_list_and_search() {
    let store = store_with(&["keep me", "remove me"]);
    let id = store.list(10).unwrap()[0].id;

    assert!(store.delete(id).unwrap());
    assert_eq!(texts(&store.list(10).unwrap()), ["keep me"]);
    assert!(store.search("remove", 10).unwrap().is_empty());
    assert!(!store.delete(id).unwrap(), "second delete finds nothing");
}

fn write_image_files(dir: &std::path::Path, name: &str) -> NewItem {
    let image = dir.join(format!("{name}.png"));
    let thumb = dir.join(format!("{name}.thumb.png"));
    std::fs::write(&image, b"png").unwrap();
    std::fs::write(&thumb, b"thumb").unwrap();
    NewItem::image(&image, &thumb, hash_bytes(name.as_bytes()), Some("paint"))
}

#[test]
fn image_items_store_paths_and_are_not_matched_by_text_search() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open_in_memory().unwrap();
    let new = write_image_files(dir.path(), "shot");
    store.insert(&new, 5).unwrap();

    let item = &store.list(10).unwrap()[0];
    assert_eq!(item.kind, Kind::Image);
    assert_eq!(item.text, None);
    assert_eq!(item.image_path, new.image_path);
    assert_eq!(item.thumb_path, new.thumb_path);
    assert!(store.search("shot", 10).unwrap().is_empty());
}

#[test]
fn deleting_an_image_removes_its_files() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open_in_memory().unwrap();
    let new = write_image_files(dir.path(), "shot");
    let (id, _) = store.insert(&new, 5).unwrap();

    assert!(store.delete(id).unwrap());
    assert!(!std::path::Path::new(new.image_path.as_ref().unwrap()).exists());
    assert!(!std::path::Path::new(new.thumb_path.as_ref().unwrap()).exists());
}

#[test]
fn delete_still_works_when_image_files_are_already_gone() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open_in_memory().unwrap();
    let new = write_image_files(dir.path(), "shot");
    let (id, _) = store.insert(&new, 5).unwrap();
    std::fs::remove_file(new.image_path.as_ref().unwrap()).unwrap();

    assert!(store.delete(id).unwrap());
    assert!(store.list(10).unwrap().is_empty());
}

#[test]
fn bump_hash_updates_existing_item_only() {
    let store = store_with(&["a1", "b1"]);
    assert_eq!(store.bump_hash(&hash_bytes(b"missing"), 9).unwrap(), None);

    let id = store.bump_hash(&hash_bytes(b"a1"), 9).unwrap().unwrap();
    let item = store.get(id).unwrap().unwrap();
    assert_eq!(item.last_used_at, 9);
    assert_eq!(texts(&store.list(10).unwrap()), ["a1", "b1"]);
}

#[test]
fn get_returns_none_for_missing_id() {
    let store = Store::open_in_memory().unwrap();
    assert_eq!(store.get(7).unwrap(), None);
}

#[test]
fn touch_moves_item_to_top() {
    let store = store_with(&["first", "second"]);
    let first = store.list(10).unwrap()[1].id;
    assert!(store.touch(first, 50).unwrap());
    assert_eq!(texts(&store.list(10).unwrap()), ["first", "second"]);
    assert!(!store.touch(999, 50).unwrap());
}

#[test]
fn clear_unpinned_keeps_pinned_and_removes_image_files() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with(&["pin me", "drop me"]);
    let pin_id = store.list(10).unwrap()[1].id;
    store.toggle_pin(pin_id).unwrap();
    let image = write_image_files(dir.path(), "img");
    store.insert(&image, 20).unwrap();

    assert_eq!(store.clear_unpinned().unwrap(), 2);
    assert_eq!(texts(&store.list(10).unwrap()), ["pin me"]);
    assert!(!std::path::Path::new(image.image_path.as_ref().unwrap()).exists());
}

#[test]
fn settings_values_round_trip_and_overwrite() {
    let store = Store::open_in_memory().unwrap();
    assert_eq!(store.get_setting("k").unwrap(), None);
    store.set_setting("k", "1").unwrap();
    store.set_setting("k", "2").unwrap();
    assert_eq!(store.get_setting("k").unwrap().as_deref(), Some("2"));
}
