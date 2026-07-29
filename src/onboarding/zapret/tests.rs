use std::fs;

use super::*;

#[test]
fn flowseal_root_points_only_to_user_list() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join("bin")).unwrap();
    fs::create_dir_all(temp.path().join("lists")).unwrap();
    fs::write(temp.path().join("bin").join("winws.exe"), b"stub").unwrap();

    let install = ZapretInstall::detect(temp.path()).unwrap();

    assert_eq!(install.kind, ZapretKind::FlowsealWindows);
    assert_eq!(
        install.list_path,
        temp.path().join("lists").join("list-general-user.txt")
    );
}

#[test]
fn full_flowseal_list_path_is_accepted_without_guessing_another_file() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join("bin")).unwrap();
    fs::create_dir_all(temp.path().join("lists")).unwrap();
    fs::write(temp.path().join("bin").join("winws.exe"), b"stub").unwrap();
    let list = temp.path().join("lists").join("list-general-user.txt");

    let install = ZapretInstall::detect(&list).unwrap();

    assert_eq!(install.root, temp.path());
    assert_eq!(install.list_path, list);
}

#[test]
fn snowy_root_needs_the_real_zapret_shape() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join("ipset")).unwrap();
    fs::create_dir_all(temp.path().join("nfq")).unwrap();

    let install = ZapretInstall::detect(temp.path()).unwrap();

    assert_eq!(install.kind, ZapretKind::SnowyLinux);
    assert_eq!(
        install.list_path,
        temp.path().join("ipset").join("zapret-hosts-user.txt")
    );
}

#[test]
fn random_folder_with_a_lists_directory_gets_nothing() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join("lists")).unwrap();

    let error = ZapretInstall::detect(temp.path()).unwrap_err();

    assert!(error.to_string().contains("не похож"));
}
