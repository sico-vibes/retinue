//! Phase 9 storage tests: project memberships, the preset link, the CEO rename
//! and the CEO playbook. Included as `mod phase9_tests` from `lib.rs`.

use super::*;
use serde_json::json;

fn runtime(db: &Storage) {
    db.upsert_runtime(&json!({"id":"rt-9","provider":"codex","status":"offline"})).unwrap();
}

fn agent(db: &Storage, name: &str) -> String {
    db.agent_create(&json!({"name":name,"runtimeId":"rt-9"})).unwrap().0["id"].as_str().unwrap().to_owned()
}

/// A conversation in `project_id`, idle, so the backfill sees it as live work.
fn conversation(db: &Storage, session: &str, agent_id: &str, project_id: &str) {
    db.create_session_for_agent(session, "rt-9", "codex", ".", "Work", Some(agent_id)).unwrap();
    db.update_session_state(session, "idle").unwrap();
    db.merge_session_data(session, &json!({"projectId": project_id})).unwrap();
}

/// Forget a one-time marker, as a database written before Phase 9 would not have it.
fn forget_marker(db: &Storage, key: &str) {
    db.conn.lock().unwrap().execute("DELETE FROM settings WHERE key=?1", [key]).unwrap();
}

fn run_data_step(db: &Storage) {
    let c = db.conn.lock().unwrap();
    crate::phase9::ensure_phase9_data(&c).unwrap();
}

#[test]
fn an_employee_keeps_a_validated_personality() {
    let db = Storage::open_in_memory().unwrap();
    runtime(&db);
    let (created, _) = db.agent_create(&json!({"name":"Pat","runtimeId":"rt-9","personality":{"id":"warm","prompt":"  Be kind.  "}})).unwrap();
    assert_eq!(created["personality"], json!({"id":"warm","prompt":"Be kind."}));
    let id = created["id"].as_str().unwrap();
    assert!(db.agent_create(&json!({"name":"Bad","runtimeId":"rt-9","personality":{"id":"Warm","prompt":"x"}})).is_err(), "ids are lowercase");
    assert!(db.agent_create(&json!({"name":"Bad2","runtimeId":"rt-9","personality":{"id":"warm","prompt":"x".repeat(2001)}})).is_err());
    assert!(db.agent_create(&json!({"name":"Bad3","runtimeId":"rt-9","personality":{"id":"warm"}})).is_err(), "a prompt is required in storage");
    // A plain update leaves it alone; `agent_set_personality` changes and clears it.
    db.agent_update(id, &json!({"description":"Still Pat."})).unwrap();
    assert_eq!(db.agent_get(id).unwrap()["personality"]["prompt"], "Be kind.");
    let (changed, events) = db.agent_set_personality(id, &json!({"id":"custom","prompt":"Be brief."})).unwrap();
    assert_eq!(changed["personality"], json!({"id":"custom","prompt":"Be brief."}));
    assert_eq!(events.len(), 1);
    assert!(db.agent_set_personality(id, &json!({"id":"custom"})).is_err());
    let (cleared, _) = db.agent_set_personality(id, &Value::Null).unwrap();
    assert!(cleared["personality"].is_null());
    assert!(db.agent_list(false, None).unwrap().iter().all(|agent| agent.get("personality").is_some()), "the key is always present");
}

fn members(db: &Storage, project_id: &str) -> Vec<String> {
    db.project_members(project_id).unwrap().iter().filter_map(|row| row["agentId"].as_str().map(str::to_owned)).collect()
}

#[test]
fn the_backfill_seeds_memberships_from_default_projects_and_conversations() {
    let db = Storage::open_in_memory().unwrap();
    runtime(&db);
    let work = db.project_create("Work", None).unwrap().0["id"].as_str().unwrap().to_owned();
    let other = db.project_create("Other", None).unwrap().0["id"].as_str().unwrap().to_owned();
    let ada = agent(&db, "Ada");
    let lan = agent(&db, "Lan");
    // Ada's default project is Work, and she has a conversation in Other.
    db.agent_set_team(&ada, &json!({"projectId":work})).unwrap();
    conversation(&db, "ada-other", &ada, &other);
    // Lan only works through a conversation.
    conversation(&db, "lan-work", &lan, &work);
    // The CEO is in a project by conversation and still gets no row.
    let boss = db.agent_create(&json!({"name":"CEO","runtimeId":"rt-9","systemRole":"manager","role":"CEO"})).unwrap().0["id"].as_str().unwrap().to_owned();
    conversation(&db, "ceo-work", &boss, &work);
    forget_marker(&db, "migration.projectMembersSeeded");
    run_data_step(&db);
    let mut seeded = members(&db, &work);
    seeded.sort();
    let mut expected = vec![ada.clone(), lan.clone()];
    expected.sort();
    assert_eq!(seeded, expected, "the default project and a conversation both seed a membership, and the CEO is skipped");
    assert_eq!(members(&db, &other), vec![ada.clone()]);
    let ada_work = db.project_member(&work, &ada).unwrap().expect("Ada is hired into Work");
    assert!(ada_work["hiredAt"].as_i64().is_some_and(|at| at > 0), "hired_at is Unix milliseconds");
    assert!(ada_work["sortOrder"].as_i64().is_some_and(|order| order >= 0));
    // Running it again changes nothing.
    let before = db.project_members(&work).unwrap();
    forget_marker(&db, "migration.projectMembersSeeded");
    run_data_step(&db);
    assert_eq!(db.project_members(&work).unwrap(), before, "a second run changes nothing");
}

#[test]
fn hiring_releasing_and_deleting_keep_the_team_true() {
    let db = Storage::open_in_memory().unwrap();
    runtime(&db);
    let work = db.project_create("Work", None).unwrap().0["id"].as_str().unwrap().to_owned();
    let other = db.project_create("Other", None).unwrap().0["id"].as_str().unwrap().to_owned();
    let ada = agent(&db, "Ada");
    db.agent_set_team(&ada, &json!({"projectId":work})).unwrap();
    let (member, created) = db.project_add_member(&work, &ada).unwrap();
    assert!(created);
    assert_eq!(member["agentId"], ada.as_str());
    // Hiring twice is one row, unchanged.
    let (again, created) = db.project_add_member(&work, &ada).unwrap();
    assert!(!created);
    assert_eq!(again, member);
    assert_eq!(db.project_members(&work).unwrap().len(), 1);
    // A second project is a second membership for the same bot.
    assert!(db.project_add_member(&other, &ada).unwrap().1);
    assert_eq!(db.project_ids_for_agent(&ada).unwrap().len(), 2);
    // Releasing one project clears a default pointing at it and keeps the bot.
    assert!(db.project_release(&work, &ada).unwrap());
    assert!(members(&db, &work).is_empty());
    assert_eq!(db.agent_get(&ada).unwrap()["projectId"], Value::Null);
    assert_eq!(db.project_ids_for_agent(&ada).unwrap(), vec![other.clone()], "the other membership stays");
    assert!(!db.project_release(&work, &ada).unwrap(), "releasing twice reports nothing removed");
    // The released project drops out of agent.projects; the remaining one stays.
    let ids: Vec<String> = db.agent_projects(&ada).unwrap().iter().filter_map(|row| row["id"].as_str().map(str::to_owned)).collect();
    assert_eq!(ids, vec![other.clone()]);
    // A project with no conversation still counts, with nothing in it.
    let listed = db.agent_projects(&ada).unwrap();
    assert_eq!(listed[0]["conversationCount"], 0);
    // Deleting a project takes its team rows with it.
    assert!(db.project_add_member(&work, &ada).unwrap().1);
    db.project_delete(&work).unwrap();
    assert!(members(&db, &work).is_empty());
    // Purging a bot takes every row it holds.
    db.agent_purge(&ada).unwrap();
    let left: i64 = db.conn.lock().unwrap().query_row("SELECT COUNT(*) FROM project_members WHERE agent_id=?1", [ada.as_str()], |r| r.get(0)).unwrap();
    assert_eq!(left, 0);
}

#[test]
fn the_preset_link_is_unique_and_freed_by_archiving() {
    let db = Storage::open_in_memory().unwrap();
    runtime(&db);
    let ada = agent(&db, "Ada");
    db.agent_set_preset(&ada, Some("ada")).unwrap();
    assert_eq!(db.agent_with_preset("ada").unwrap().unwrap()["id"], ada.as_str());
    assert_eq!(db.preset_agents().unwrap().len(), 1);
    // Only one live bot may carry a preset.
    let second = agent(&db, "Ada 2");
    assert!(db.agent_set_preset(&second, Some("ada")).is_err(), "the partial unique index refuses a second one");
    // Archiving frees the preset, so hiring it again makes a fresh employee.
    db.agent_archive(&ada).unwrap();
    assert!(db.agent_with_preset("ada").unwrap().is_none());
    db.agent_set_preset(&second, Some("ada")).unwrap();
    assert_eq!(db.agent_with_preset("ada").unwrap().unwrap()["id"], second.as_str());
    // Clearing it releases the preset for good.
    db.agent_set_preset(&second, None).unwrap();
    assert!(db.agent_with_preset("ada").unwrap().is_none());
}

#[test]
fn the_ceo_rename_only_touches_the_default_name() {
    // The default name becomes CEO.
    let db = Storage::open_in_memory().unwrap();
    runtime(&db);
    let boss = db.agent_create(&json!({"name":"Manager","runtimeId":"rt-9","systemRole":"manager","role":"Manager"})).unwrap().0["id"].as_str().unwrap().to_owned();
    forget_marker(&db, "migration.ceoRename");
    run_data_step(&db);
    let renamed = db.agent_get(&boss).unwrap();
    assert_eq!((renamed["name"].as_str(), renamed["role"].as_str()), (Some("CEO"), Some("CEO")));
    assert_eq!(renamed["systemRole"], "manager", "the system role and the RPC names stay");
    // A name the owner chose stays; only the role label follows the default.
    let db = Storage::open_in_memory().unwrap();
    runtime(&db);
    let boss = db.agent_create(&json!({"name":"Chief","runtimeId":"rt-9","systemRole":"manager","role":"Manager"})).unwrap().0["id"].as_str().unwrap().to_owned();
    forget_marker(&db, "migration.ceoRename");
    run_data_step(&db);
    let kept = db.agent_get(&boss).unwrap();
    assert_eq!((kept["name"].as_str(), kept["role"].as_str()), (Some("Chief"), Some("CEO")));
    // A bot the owner already named CEO blocks the rename instead of colliding.
    let db = Storage::open_in_memory().unwrap();
    runtime(&db);
    agent(&db, "CEO");
    let boss = db.agent_create(&json!({"name":"Manager","runtimeId":"rt-9","systemRole":"manager","role":"Manager"})).unwrap().0["id"].as_str().unwrap().to_owned();
    forget_marker(&db, "migration.ceoRename");
    run_data_step(&db);
    assert_eq!(db.agent_get(&boss).unwrap()["name"], "Manager", "two active bots cannot share a name");
}

#[test]
fn the_untouched_default_playbook_becomes_the_ceo_one() {
    let db = Storage::open_in_memory().unwrap();
    assert_eq!(db.manager_playbook_current().unwrap()["text"], DEFAULT_PLAYBOOK.trim());
    // An install still on the old default is moved to the new one.
    db.conn.lock().unwrap().execute("UPDATE manager_playbook SET text=?1 WHERE version=1", [DEFAULT_PLAYBOOK_V1.trim()]).unwrap();
    forget_marker(&db, "migration.ceoPlaybook");
    run_data_step(&db);
    let playbook = db.manager_playbook_current().unwrap();
    assert_eq!(playbook["text"], DEFAULT_PLAYBOOK.trim());
    assert_eq!(playbook["author"], "default");
    assert!(playbook["version"].as_i64().unwrap() > 1, "a new version, so the history is kept");
    // A playbook the owner edited is left exactly as it is.
    let db = Storage::open_in_memory().unwrap();
    db.manager_playbook_append("# Ours\n\nOur own rules.", "user").unwrap();
    forget_marker(&db, "migration.ceoPlaybook");
    run_data_step(&db);
    assert_eq!(db.manager_playbook_current().unwrap()["text"], "# Ours\n\nOur own rules.");
}

#[test]
fn the_delegating_section_reaches_an_untouched_default_and_not_an_edited_one() {
    assert!(DEFAULT_PLAYBOOK.contains("## Delegating"));
    assert!(!DEFAULT_PLAYBOOK_V2.contains("## Delegating"));
    let db = Storage::open_in_memory().unwrap();
    // An install still on the previous default is moved to the new text.
    db.conn.lock().unwrap().execute("UPDATE manager_playbook SET text=?1 WHERE version=1", [DEFAULT_PLAYBOOK_V2.trim()]).unwrap();
    forget_marker(&db, "migration.delegatingPlaybook");
    run_data_step(&db);
    let playbook = db.manager_playbook_current().unwrap();
    assert_eq!(playbook["text"], DEFAULT_PLAYBOOK.trim());
    assert_eq!(playbook["author"], "default");
    assert!(playbook["version"].as_i64().unwrap() > 1, "a new version, so the history is kept");
    // A playbook the owner edited is left exactly as it is.
    let db = Storage::open_in_memory().unwrap();
    db.manager_playbook_append("# Ours\n\nOur own rules.", "user").unwrap();
    forget_marker(&db, "migration.delegatingPlaybook");
    run_data_step(&db);
    assert_eq!(db.manager_playbook_current().unwrap()["text"], "# Ours\n\nOur own rules.");
}
