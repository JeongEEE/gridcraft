use gridcraft_update::install::{extract_portable, replace_bundle, replace_portable};
use std::io::Write;

#[test]
fn portable_keeps_user_files_and_restores_shipped_files_if_restart_fails() {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("Grid");
    let staged = root.path().join("new");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::create_dir_all(&staged).unwrap();
    std::fs::write(target.join("grid.exe"), "old").unwrap();
    std::fs::write(target.join("book.xlsx"), "user workbook").unwrap();
    std::fs::write(staged.join("grid.exe"), "new").unwrap();
    std::fs::write(staged.join("grid-cli.exe"), "new cli").unwrap();
    let result = replace_portable(&staged, &target, || {
        assert_eq!(std::fs::read_to_string(target.join("grid.exe")).unwrap(), "new");
        Err("restart failed".into())
    });
    assert!(result.is_err());
    assert_eq!(std::fs::read_to_string(target.join("grid.exe")).unwrap(), "old");
    assert!(!target.join("grid-cli.exe").exists());
    assert_eq!(std::fs::read_to_string(target.join("book.xlsx")).unwrap(), "user workbook");
}

#[test]
fn portable_installs_all_files_without_removing_personal_data() {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("Grid");
    let staged = root.path().join("new");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::create_dir_all(&staged).unwrap();
    std::fs::write(target.join("grid.exe"), "old").unwrap();
    std::fs::write(target.join("notes.txt"), "keep").unwrap();
    std::fs::write(staged.join("grid.exe"), "new").unwrap();
    std::fs::write(staged.join("grid-cli.exe"), "new cli").unwrap();
    replace_portable(&staged, &target, || Ok(())).unwrap();
    assert_eq!(std::fs::read_to_string(target.join("grid.exe")).unwrap(), "new");
    assert_eq!(std::fs::read_to_string(target.join("grid-cli.exe")).unwrap(), "new cli");
    assert_eq!(std::fs::read_to_string(target.join("notes.txt")).unwrap(), "keep");
}

#[test]
fn bundle_swap_rolls_back_when_new_application_cannot_start() {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("Grid.app");
    let staged = root.path().join("next.app");
    std::fs::create_dir(&target).unwrap();
    std::fs::create_dir(&staged).unwrap();
    std::fs::write(target.join("version"), "old").unwrap();
    std::fs::write(staged.join("version"), "new").unwrap();
    assert!(replace_bundle(&staged, &target, || Err("new app crashed".into())).is_err());
    assert_eq!(std::fs::read_to_string(target.join("version")).unwrap(), "old");
}

fn archive(name: &str) -> tempfile::NamedTempFile {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    {
        let mut zip = zip::ZipWriter::new(&mut file);
        zip.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(b"fake exe").unwrap();
        zip.finish().unwrap();
    }
    file
}

#[test]
fn portable_zip_refuses_paths_outside_its_single_release_directory() {
    let root = tempfile::tempdir().unwrap();
    for name in ["../grid.exe", "Grid/../../grid.exe", "/grid.exe", "Grid/C:/grid.exe", "Grid/grid.exe:evil", "Grid/CON", "Grid/grid.exe."] {
        let file = archive(name);
        assert!(extract_portable(file.path(), &root.path().join("out"), "0.19.3").is_err(), "{name}");
    }
    assert!(!root.path().join("grid.exe").exists());
}
