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

#[test]
fn plan_adds_only_missing_soundcloud_roots() {
    let temp = flowseal_install();
    let list = temp.path().join("lists").join("list-general-user.txt");
    fs::write(&list, "example.org\nsoundcloud.com # уже есть\n").unwrap();

    let plan = ZapretPlan::build(ZapretInstall::detect(temp.path()).unwrap()).unwrap();

    assert_eq!(plan.additions, vec!["sndcdn.com"]);
    assert!(plan.render_diff().contains("+ sndcdn.com"));
    assert!(!plan.render_diff().contains("+ soundcloud.com"));
}

#[test]
fn finished_plan_is_idempotent() {
    let temp = flowseal_install();
    let list = temp.path().join("lists").join("list-general-user.txt");
    fs::write(&list, "soundcloud.com\nsndcdn.com\n").unwrap();

    let plan = ZapretPlan::build(ZapretInstall::detect(temp.path()).unwrap()).unwrap();

    assert!(!plan.has_changes());
    assert!(plan.render_diff().contains("изменений нет"));
}

#[test]
fn plan_preserves_windows_line_endings() {
    let temp = flowseal_install();
    let list = temp.path().join("lists").join("list-general-user.txt");
    fs::write(&list, "example.org\r\n").unwrap();

    let plan = ZapretPlan::build(ZapretInstall::detect(temp.path()).unwrap()).unwrap();

    assert_eq!(
        plan.resulting_contents(),
        "example.org\r\nsoundcloud.com\r\nsndcdn.com\r\n"
    );
}

#[test]
fn apply_keeps_backup_and_writes_domains_once() {
    let temp = flowseal_install();
    let list = temp.path().join("lists").join("list-general-user.txt");
    fs::write(&list, "example.org\n").unwrap();
    let plan = ZapretPlan::build(ZapretInstall::detect(temp.path()).unwrap()).unwrap();

    let result = apply_plan(&plan).unwrap();

    assert_eq!(
        fs::read_to_string(&list).unwrap(),
        plan.resulting_contents()
    );
    assert_eq!(
        fs::read_to_string(result.backup_path.unwrap()).unwrap(),
        "example.org\n"
    );
    let second = ZapretPlan::build(ZapretInstall::detect(temp.path()).unwrap()).unwrap();
    assert!(!second.has_changes());
}

#[test]
fn stale_plan_refuses_to_overwrite_someone_elses_edit() {
    let temp = flowseal_install();
    let list = temp.path().join("lists").join("list-general-user.txt");
    fs::write(&list, "example.org\n").unwrap();
    let plan = ZapretPlan::build(ZapretInstall::detect(temp.path()).unwrap()).unwrap();
    fs::write(&list, "example.net\n").unwrap();

    let error = apply_plan(&plan).unwrap_err();

    assert!(error.to_string().contains("изменился после показа diff"));
    assert_eq!(fs::read_to_string(list).unwrap(), "example.net\n");
}

#[test]
fn new_user_list_needs_no_fake_backup() {
    let temp = flowseal_install();
    let plan = ZapretPlan::build(ZapretInstall::detect(temp.path()).unwrap()).unwrap();

    let result = apply_plan(&plan).unwrap();

    assert!(result.backup_path.is_none());
    assert_eq!(
        fs::read_to_string(result.list_path).unwrap(),
        "soundcloud.com\nsndcdn.com\n"
    );
}

#[test]
fn sudo_hint_quotes_a_weird_but_valid_path() {
    let command = sudo_command(std::path::Path::new("/opt/дед's zapret"));

    assert_eq!(
        command,
        "sudo noverplay setup-zapret --path '/opt/дед'\"'\"'s zapret'"
    );
}

fn flowseal_install() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join("bin")).unwrap();
    fs::create_dir_all(temp.path().join("lists")).unwrap();
    fs::write(temp.path().join("bin").join("winws.exe"), b"stub").unwrap();
    temp
}
