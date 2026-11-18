// 17 November 2026: a project started on this machine by one of the owner's paired devices.
//
// Included into `machines_tests`, so it shares that module's two-daemon helpers: `host` is the other
// machine behind its real Native gateway, `local` is this one, and `link_native` pairs them through
// the real pairing. Every method is called through `dispatch`, and every far-side rule is proved
// through the link as well as directly, because the gateway is where the tier and the activity line
// are enforced.

/// A folder under a fresh temporary root, for the host's shared folder.
fn shared_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("retinue-ws-{tag}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Turns the surface on for `st`, with `base` as the one folder it may use.
fn share(st: &AppState, base: &std::path::Path) {
    st.db.set_setting(crate::project_share::SETTING_ENABLED, &json!(true)).unwrap();
    st.db.set_setting(crate::project_share::SETTING_FOLDER, &json!(base.to_string_lossy())).unwrap();
}

#[tokio::test]
async fn browsing_is_off_until_the_owner_turns_it_on_and_names_a_folder() {
    let (st, dir) = local("ws-off");
    let base = shared_dir("ws-off-base");
    std::fs::create_dir_all(base.join("project")).unwrap();
    // Off is the default: the switch is absent, so nothing is refused quietly and nothing is listed.
    let refused = dispatch(&st, "project.browse", json!({})).await.unwrap_err();
    assert_eq!(refused.1.code, "forbidden_remote");
    assert_eq!(refused.1.message, "Projects from other machines are off on this computer. Turn them on in Settings › Machines.");
    let refused = dispatch(&st, "project.createFromFolder", json!({"path": base.join("project")})).await.unwrap_err();
    assert_eq!(refused.1.code, "forbidden_remote");
    // On, but with no folder chosen.
    st.db.set_setting(crate::project_share::SETTING_ENABLED, &json!(true)).unwrap();
    let refused = dispatch(&st, "project.browse", json!({})).await.unwrap_err();
    assert_eq!(refused.1.message, "Choose the folder other machines may use for new projects in Settings › Machines first.");
    // On with a folder that is gone says so rather than listing the disk.
    st.db.set_setting(crate::project_share::SETTING_FOLDER, &json!(base.join("gone").to_string_lossy())).unwrap();
    let refused = dispatch(&st, "project.browse", json!({})).await.unwrap_err();
    assert_eq!(refused.1.message, "The folder other machines may use for new projects is missing on this computer.");
    // A clone from a device is refused by the same gate before git is started.
    let refused = dispatch(&st, "project.clone", json!({"url":"https://example.com/x.git","parent":base,"confirm":true,"deviceCall":true})).await.unwrap_err();
    assert_eq!(refused.1.code, "forbidden_remote");
    assert!(!base.join("x").exists(), "no clone started");
    let _ = std::fs::remove_dir_all(&base);
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn browsing_lists_names_and_kinds_inside_the_folder_only() {
    let (st, dir) = local("ws-browse");
    let base = shared_dir("ws-browse-base");
    std::fs::create_dir_all(base.join("work").join("nested")).unwrap();
    std::fs::write(base.join("notes.txt"), "contents that must never be listed").unwrap();
    std::fs::write(base.join("work").join("secret.txt"), "nor this one").unwrap();
    let outside = shared_dir("ws-browse-outside");
    share(&st, &base);

    // The base folder itself: its two children by name and kind, no contents and no sizes.
    let listed = dispatch(&st, "project.browse", json!({})).await.unwrap();
    assert_eq!(listed["path"], json!(std::fs::canonicalize(&base).unwrap().to_string_lossy()));
    assert_eq!(listed["parent"], Value::Null, "the base folder has no parent to go to");
    assert_eq!(listed["entries"], json!([{"name":"notes.txt","kind":"file"},{"name":"work","kind":"folder"}]));
    assert!(!listed.to_string().contains("contents that must never be listed"), "no file contents cross");
    assert!(!listed.to_string().contains("secret.txt"), "a listing is one level");

    // A folder inside it, named as the reply above named it.
    let inside = listed["path"].as_str().unwrap().to_string();
    let child = std::path::Path::new(&inside).join("work");
    let listed = dispatch(&st, "project.browse", json!({"path": child})).await.unwrap();
    assert_eq!(listed["entries"], json!([{"name":"nested","kind":"folder"},{"name":"secret.txt","kind":"file"}]));
    assert_eq!(listed["parent"], json!(std::fs::canonicalize(&base).unwrap().to_string_lossy()));

    // `..`, a path beside the base, a relative path and a file are all refused, with the same
    // sentence where the path is the problem.
    let escape = std::path::Path::new(&inside).join("..");
    for (path, message) in [
        (escape.to_string_lossy().into_owned(), "That folder is outside the folder this computer shares with other machines."),
        (outside.to_string_lossy().into_owned(), "That folder is outside the folder this computer shares with other machines."),
        ("work".to_string(), "That folder is outside the folder this computer shares with other machines."),
        (std::path::Path::new(&inside).join("notes.txt").to_string_lossy().into_owned(), "That folder is not on this computer."),
        (std::path::Path::new(&inside).join("missing").to_string_lossy().into_owned(), "That folder is not on this computer."),
    ] {
        let refused = dispatch(&st, "project.browse", json!({"path": path})).await.unwrap_err();
        assert_eq!(refused.1.code, "forbidden_remote", "{path}");
        assert_eq!(refused.1.message, message, "{path}");
    }
    let _ = std::fs::remove_dir_all(&base);
    let _ = std::fs::remove_dir_all(&outside);
    let _ = std::fs::remove_dir_all(dir);
}

/// A junction out of the shared folder is neither listed nor browsable (Windows only; creating one
/// needs no elevation, but a locked-down machine can refuse it, so the test skips that half).
#[cfg(windows)]
#[tokio::test]
async fn a_junction_out_of_the_folder_is_neither_listed_nor_browsed() {
    let (st, dir) = local("ws-junction");
    let base = shared_dir("ws-junction-base");
    let outside = shared_dir("ws-junction-outside");
    std::fs::write(outside.join("private.txt"), "outside").unwrap();
    let link = base.join("escape");
    let made = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J", &link.to_string_lossy(), &outside.to_string_lossy()])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);
    share(&st, &base);
    if made {
        let listed = dispatch(&st, "project.browse", json!({})).await.unwrap();
        assert_eq!(listed["entries"], json!([]), "a junction out of the folder is not offered: {listed}");
        let refused = dispatch(&st, "project.browse", json!({"path": link.to_string_lossy()})).await.unwrap_err();
        assert_eq!(refused.1.message, "That folder is outside the folder this computer shares with other machines.");
        let refused = dispatch(&st, "project.createFromFolder", json!({"path": link.to_string_lossy()})).await.unwrap_err();
        assert_eq!(refused.1.message, "That folder is outside the folder this computer shares with other machines.");
        // The desktop's own clone is not confined, but a device's is: the same junction as a parent.
        let refused = dispatch(&st, "project.clone", json!({"url":"https://example.com/x.git","parent":link,"confirm":true,"deviceCall":true})).await.unwrap_err();
        assert_eq!(refused.1.message, "That folder is outside the folder this computer shares with other machines.");
    }
    let _ = std::fs::remove_dir_all(&base);
    let _ = std::fs::remove_dir_all(&outside);
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn a_folder_inside_becomes_a_project_and_one_outside_is_refused() {
    let (st, dir) = local("ws-create");
    let base = shared_dir("ws-create-base");
    let folder = base.join("checkout");
    std::fs::create_dir_all(&folder).unwrap();
    let outside = shared_dir("ws-create-outside");
    share(&st, &base);
    let made = dispatch(&st, "project.createFromFolder", json!({"path": folder})).await.unwrap();
    assert_eq!(made["project"]["name"], "checkout", "the folder's own name is the default");
    assert_eq!(made.as_object().unwrap()["project"].as_object().unwrap().len(), 3, "only id, name and path cross: {made}");
    let project_id = made["project"]["id"].as_str().unwrap().to_string();
    let listed = dispatch(&st, "project.list", json!({})).await.unwrap();
    assert!(listed["projects"].as_array().unwrap().iter().any(|project| project["id"] == project_id.as_str()));
    // A name the owner chose is kept.
    let named = dispatch(&st, "project.createFromFolder", json!({"path": folder, "name": "Checkout copy"})).await.unwrap();
    assert_eq!(named["project"]["name"], "Checkout copy");
    // Outside the shared folder, and a file, are refused.
    let refused = dispatch(&st, "project.createFromFolder", json!({"path": outside})).await.unwrap_err();
    assert_eq!(refused.1.code, "forbidden_remote");
    let file = base.join("readme.txt");
    std::fs::write(&file, "x").unwrap();
    let refused = dispatch(&st, "project.createFromFolder", json!({"path": file})).await.unwrap_err();
    assert_eq!(refused.1.message, "That folder is not on this computer.");
    let _ = std::fs::remove_dir_all(&base);
    let _ = std::fs::remove_dir_all(&outside);
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn a_device_clone_is_confined_and_reports_its_progress_and_a_failure() {
    let Some(repo) = crate::phase2a_tests::LocalRepo::real("ws-clone").await else { return };
    let (st, dir) = local("ws-clone");
    let base = shared_dir("ws-clone-base");
    let outside = shared_dir("ws-clone-outside");
    share(&st, &base);
    let url = repo.path.to_string_lossy().into_owned();

    // Outside the shared folder a device's clone is refused before git runs.
    let refused = dispatch(&st, "project.clone", json!({"url":url,"parent":outside,"confirm":true,"deviceCall":true})).await.unwrap_err();
    assert_eq!(refused.1.code, "forbidden_remote");
    assert!(!outside.join("source").exists());
    // The desktop (no stamp) may still clone into any folder of its choosing, switch on or off.
    let desktop = dispatch(&st, "project.clone", json!({"url":url,"parent":outside,"confirm":true})).await.unwrap();
    let desktop_job = desktop["job"]["id"].as_str().unwrap().to_string();

    let started = dispatch(&st, "project.clone", json!({"url":url,"parent":base,"confirm":true,"name":"mine","deviceCall":true})).await.unwrap();
    let id = started["job"]["id"].as_str().unwrap().to_string();
    assert_eq!(started["job"]["target"], json!(base.join("mine").to_string_lossy()), "the target sits in the shared folder");

    // Progress: `running` until the clone finishes, then `done` with the project by id and name only.
    let mut seen_running = false;
    let done = loop {
        let job = dispatch(&st, "project.clone.status", json!({"id": id})).await.unwrap();
        let state = job["job"]["state"].as_str().unwrap().to_string();
        if state == "running" {
            seen_running = true;
        } else {
            break job;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    };
    assert!(seen_running, "a clone reports itself as running first");
    assert_eq!(done["job"]["state"], "done", "{done}");
    assert_eq!(done["job"]["project"]["name"], "mine");
    assert_eq!(done["job"]["project"].as_object().unwrap().len(), 2, "the project row is reduced: {done}");
    assert_eq!(done["job"]["detail"], Value::Null);
    // An unknown job, and an empty one, are plain refusals.
    assert_eq!(dispatch(&st, "project.clone.status", json!({"id": "nope"})).await.unwrap_err().1.code, "not_found");
    assert_eq!(dispatch(&st, "project.clone.status", json!({"id": ""})).await.unwrap_err().1.code, "invalid_argument");
    // The desktop's own job finished too.
    let desktop = dispatch(&st, "project.clone.status", json!({"id": desktop_job})).await.unwrap();
    assert_eq!(desktop["job"]["state"], "done");

    // A failure: the same test-only allowance pointed at a folder that holds no repository. The lock
    // that guards the allowance is one at a time, so the first repository is released first.
    drop(repo);
    let missing = crate::phase2a_tests::LocalRepo::missing("ws-clone-missing").await;
    let failed = dispatch(&st, "project.clone", json!({"url": missing.path.to_string_lossy(),"parent":base,"confirm":true,"name":"broken","deviceCall":true})).await.unwrap();
    let failed_id = failed["job"]["id"].as_str().unwrap().to_string();
    let failed = loop {
        let job = dispatch(&st, "project.clone.status", json!({"id": failed_id})).await.unwrap();
        if job["job"]["state"] != "running" {
            break job;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    };
    assert_eq!(failed["job"]["state"], "failed", "{failed}");
    assert!(failed["job"]["detail"].as_str().is_some_and(|line| !line.is_empty() && !line.contains('\n')), "one plain line: {failed}");
    assert!(!base.join("broken").exists(), "a failed clone removes the folder it made");
    drop(missing);
    let _ = std::fs::remove_dir_all(&base);
    let _ = std::fs::remove_dir_all(&outside);
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn the_settings_are_held_to_their_shape_and_never_travel_in_a_backup() {
    let (st, dir) = local("ws-settings");
    for value in [json!("yes"), json!(1)] {
        let refused = dispatch(&st, "settings.set", json!({"key": crate::project_share::SETTING_ENABLED, "value": value})).await.unwrap_err();
        assert_eq!(refused.1.code, "invalid_argument");
    }
    for value in [json!("relative/folder"), json!(5)] {
        let refused = dispatch(&st, "settings.set", json!({"key": crate::project_share::SETTING_FOLDER, "value": value})).await.unwrap_err();
        assert_eq!(refused.1.code, "invalid_argument");
    }
    let base = shared_dir("ws-settings-base");
    dispatch(&st, "settings.set", json!({"key": crate::project_share::SETTING_ENABLED, "value": true})).await.unwrap();
    dispatch(&st, "settings.set", json!({"key": crate::project_share::SETTING_FOLDER, "value": base.to_string_lossy()})).await.unwrap();
    let settings = dispatch(&st, "settings.get", json!({})).await.unwrap();
    assert_eq!(settings[crate::project_share::SETTING_ENABLED], true);
    // A backup carries neither: they belong to this machine, like `projects.folder`.
    let exported = dispatch(&st, "backup.export", json!({})).await.unwrap().to_string();
    assert!(!exported.contains("projects.devices"), "the switch and the folder are this machine's");
    let _ = std::fs::remove_dir_all(&base);
    let _ = std::fs::remove_dir_all(dir);
}

// ---------------------------------------------------------------------------
// Through the link: the window on this machine asks the other one.

/// A host with the surface switched on over a fresh folder, paired to a fresh `local` at `tier`.
async fn shared_host(tag: &str, tier: &str) -> (Host, PathBuf, AppState, PathBuf) {
    let other = host(&format!("{tag}-host")).await;
    let base = shared_dir(&format!("{tag}-base"));
    share(&other.st, &base);
    let (me, my_dir) = local(&format!("{tag}-me"));
    link_native(&me, &other, tier).await;
    (other, base, me, my_dir)
}

#[tokio::test]
async fn a_link_browses_and_adds_a_project_inside_the_folder_only() {
    let (other, base, me, my_dir) = shared_host("ws-link", "operator").await;
    let other_id = host_id(&other);
    let folder = base.join("checkout");
    std::fs::create_dir_all(&folder).unwrap();
    // Browse: the far side's own reply, unchanged.
    let listed = dispatch(&me, "machine.project.browse", json!({"machineId": other_id})).await.unwrap();
    assert_eq!(listed["entries"], json!([{"name":"checkout","kind":"folder"}]));
    let inside = listed["path"].as_str().unwrap().to_string();
    // Add: the project row is the far side's, reduced.
    let made = dispatch(&me, "machine.project.create", json!({"machineId": other_id, "path": std::path::Path::new(&inside).join("checkout")})).await.unwrap();
    assert_eq!(made["project"]["name"], "checkout");
    let projects = dispatch(&me, "machine.projects", json!({"machineId": other_id})).await.unwrap();
    assert!(projects["projects"].as_array().unwrap().iter().any(|project| project["name"] == "checkout"), "the snapshot was refreshed: {projects}");
    // Outside the folder, the host's own sentence comes back unchanged.
    let refused = dispatch(&me, "machine.project.browse", json!({"machineId": other_id, "path": std::env::temp_dir()})).await.unwrap_err();
    assert_eq!(refused.1.code, "forbidden_remote");
    assert_eq!(refused.1.message, "That folder is outside the folder this computer shares with other machines.");
    // One activity line per action, naming the method and nothing else.
    let activity = crate::remote::rpc(&other.st, "remote.activity.list", &json!({})).await.unwrap();
    let entries = activity["entries"].as_array().unwrap();
    let line = entries.iter().find(|entry| entry["kind"] == "workspace").expect("an activity line for the browse");
    assert!(line["summary"].as_str().unwrap().contains("project.browse"), "{line}");
    assert!(!line.to_string().contains(&inside), "the line names no folder: {line}");
    let _ = std::fs::remove_dir_all(&base);
    let _ = std::fs::remove_dir_all(my_dir);
}

#[tokio::test]
async fn a_link_at_chat_may_not_browse_and_a_link_with_the_switch_off_is_refused() {
    let (other, base, me, my_dir) = shared_host("ws-tier", "chat").await;
    let other_id = host_id(&other);
    let refused = dispatch(&me, "machine.project.browse", json!({"machineId": other_id})).await.unwrap_err();
    assert_eq!(refused.1.code, "tier_required");
    assert!(refused.1.message.starts_with("Browsing its folders on "), "{}", refused.1.message);
    assert!(refused.1.message.contains("needs Operator there, and this computer is linked as Chat."), "{}", refused.1.message);
    assert!(refused.1.message.contains("Change this computer's tier in "), "{}", refused.1.message);
    // The switch off at the far side refuses every one of the four, whatever the tier.
    other.st.db.set_setting(crate::project_share::SETTING_ENABLED, &json!(false)).unwrap();
    let (operator, op_dir) = local("ws-tier-op");
    link_native(&operator, &other, "operator").await;
    for (method, params) in [
        ("machine.project.browse", json!({"machineId": other_id})),
        ("machine.project.create", json!({"machineId": other_id, "path": base})),
        ("machine.project.clone", json!({"machineId": other_id, "url": "https://example.com/x.git", "parent": base, "confirm": true})),
        ("machine.clone.status", json!({"machineId": other_id, "jobId": "nope"})),
    ] {
        let refused = dispatch(&operator, method, params).await.unwrap_err();
        assert_eq!(refused.1.code, "forbidden_remote", "{method}");
        assert_eq!(refused.1.message, "Projects from other machines are off on this computer. Turn them on in Settings › Machines.", "{method}");
    }
    let _ = std::fs::remove_dir_all(&base);
    let _ = std::fs::remove_dir_all(my_dir);
    let _ = std::fs::remove_dir_all(op_dir);
}

#[tokio::test]
async fn a_link_clones_on_the_other_machine_and_reads_its_progress() {
    let Some(repo) = crate::phase2a_tests::LocalRepo::real("ws-link-clone").await else { return };
    let (other, base, me, my_dir) = shared_host("ws-link-clone", "operator").await;
    let other_id = host_id(&other);
    let url = repo.path.to_string_lossy().into_owned();
    // The device asks for a folder inside the shared one, by the name the far side listed.
    let started = dispatch(&me, "machine.project.clone", json!({"machineId": other_id, "url": url, "parent": base, "confirm": true, "name": "landed"})).await.unwrap();
    let job = started["job"]["id"].as_str().unwrap().to_string();
    let done = loop {
        let status = dispatch(&me, "machine.clone.status", json!({"machineId": other_id, "jobId": job})).await.unwrap();
        if status["job"]["state"] != "running" {
            break status;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    };
    assert_eq!(done["job"]["state"], "done", "{done}");
    assert_eq!(done["job"]["project"]["name"], "landed");
    assert!(base.join("landed").join("readme.txt").exists(), "the clone landed in the shared folder");
    // A refusal at the far side keeps its own words: outside the folder, the clone never starts.
    let outside = shared_dir("ws-link-clone-outside");
    let refused = dispatch(&me, "machine.project.clone", json!({"machineId": other_id, "url": url, "parent": outside, "confirm": true})).await.unwrap_err();
    assert_eq!(refused.1.message, "That folder is outside the folder this computer shares with other machines.");
    drop(repo);
    let _ = std::fs::remove_dir_all(&outside);
    let _ = std::fs::remove_dir_all(&base);
    let _ = std::fs::remove_dir_all(my_dir);
}
