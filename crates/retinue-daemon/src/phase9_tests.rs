// Phase 9, Gogo's lane: employees, hiring and the CEO.
// Included into `phase2a_tests` so it shares that module's fixtures: `state`,
// `dispatch`, `call_tool`, `tool_names`, `json!` and `Uuid`.

fn a_project(st: &AppState, name: &str) -> String {
    st.db.project_create(name, Some(".")).unwrap().0["id"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn hiring_a_preset_makes_one_employee_and_reuses_it_everywhere() {
    let (st, _events) = state();
    let one = a_project(&st, "One");
    let two = a_project(&st, "Two");
    let hired = dispatch(&st, "project.hire", json!({"projectId":one,"presetId":"ada"})).await.unwrap();
    assert_eq!(hired["created"], true);
    let agent_id = hired["agentId"].as_str().unwrap().to_owned();
    let agent = st.db.agent_get(&agent_id).unwrap();
    assert_eq!(agent["name"], "Ada");
    assert_eq!(agent["role"], "Backend engineer");
    assert_eq!(agent["description"], "Builds and changes server code, APIs and business logic, with tests.");
    assert_eq!(agent["look"], json!({"shape":"circle","icon":"char-ada-lovelace","photo":"photo-17"}));
    assert_eq!(agent["tags"], json!(["backend","api","server","database"]));
    assert_eq!(agent["color"], "violet");
    assert_eq!(agent["approvalMode"], "ask");
    assert!(agent["instructions"].as_str().unwrap().contains('\n'), "the instruction lines are joined");
    assert_eq!(agent["runtimeId"], "rt-test", "with no CEO, the first available coding agent");
    assert_eq!(st.db.project_members(&one).unwrap(), vec![hired["member"].clone()]);
    // The same preset in a second project is the same employee: two memberships, one bot.
    let again = dispatch(&st, "project.hire", json!({"projectId":two,"presetId":"ada"})).await.unwrap();
    assert_eq!(again["created"], false);
    assert_eq!(again["agentId"], hired["agentId"]);
    assert_eq!(st.db.agent_list(false, None).unwrap().len(), 1);
    assert_eq!(st.db.project_members(&one).unwrap().len(), 1);
    assert_eq!(st.db.project_members(&two).unwrap().len(), 1);
    // Hiring into the same project again changes nothing.
    let same = dispatch(&st, "project.hire", json!({"projectId":one,"presetId":"ada"})).await.unwrap();
    assert_eq!(same["created"], false);
    assert_eq!(same["member"], hired["member"]);
    assert_eq!(st.db.project_members(&one).unwrap().len(), 1);
}

#[tokio::test]
async fn a_taken_name_gets_a_number() {
    let (st, _events) = state();
    let project = a_project(&st, "Taken");
    st.db.agent_create(&json!({"name":"Ada","runtimeId":"rt-test"})).unwrap();
    let hired = dispatch(&st, "project.hire", json!({"projectId":project,"presetId":"ada"})).await.unwrap();
    assert_eq!(st.db.agent_get(hired["agentId"].as_str().unwrap()).unwrap()["name"], "Ada 2");
}

#[tokio::test]
async fn hiring_refuses_the_ceo_unknown_bots_and_bad_arguments() {
    let (st, _events) = state();
    let project = a_project(&st, "Guarded");
    let boss = dispatch(&st, "manager.ensure", json!({"runtimeId":"rt-test"})).await.unwrap()["manager"].clone();
    let boss_id = boss["id"].as_str().unwrap().to_owned();
    assert_eq!(dispatch(&st, "project.hire", json!({"projectId":project,"agentId":boss_id})).await.unwrap_err().1.code, "invalid_argument");
    assert_eq!(dispatch(&st, "project.hire", json!({"projectId":project,"agentId":Uuid::new_v4().to_string()})).await.unwrap_err().1.code, "not_found");
    assert_eq!(dispatch(&st, "project.hire", json!({"projectId":project})).await.unwrap_err().1.code, "invalid_argument");
    assert_eq!(dispatch(&st, "project.hire", json!({"projectId":project,"agentId":boss_id,"presetId":"ada"})).await.unwrap_err().1.code, "invalid_argument");
    assert_eq!(dispatch(&st, "project.hire", json!({"projectId":"missing","presetId":"ada"})).await.unwrap_err().1.code, "not_found");
    assert_eq!(dispatch(&st, "project.hire", json!({"projectId":project,"presetId":"nobody"})).await.unwrap_err().1.code, "not_found");
    assert_eq!(dispatch(&st, "project.members", json!({"projectId":"missing"})).await.unwrap_err().1.code, "not_found");
    assert_eq!(dispatch(&st, "project.release", json!({"projectId":project})).await.unwrap_err().1.code, "invalid_argument");
    // An archived employee cannot be hired.
    let old = st.db.agent_create(&json!({"name":"Old","runtimeId":"rt-test"})).unwrap().0["id"].as_str().unwrap().to_owned();
    st.db.agent_archive(&old).unwrap();
    assert_eq!(dispatch(&st, "project.hire", json!({"projectId":project,"agentId":old})).await.unwrap_err().1.code, "not_found");
    assert!(st.db.project_members(&project).unwrap().is_empty());
}

#[tokio::test]
async fn releasing_keeps_conversations_and_clears_a_matching_default_project() {
    let (st, _events) = state();
    let project = a_project(&st, "Release");
    let agent = st.db.agent_create(&json!({"name":"Ada","runtimeId":"rt-test"})).unwrap().0["id"].as_str().unwrap().to_owned();
    // A default project is a team placement: it hires the bot.
    dispatch(&st, "agent.team.update", json!({"agentId":agent,"projectId":project})).await.unwrap();
    assert_eq!(st.db.project_members(&project).unwrap().len(), 1, "a default project hires its bot");
    assert_eq!(st.db.agent_get(&agent).unwrap()["projectId"], project.as_str());
    let created = dispatch(&st, "agent.conversation.create", json!({"agentId":agent,"projectId":project})).await.unwrap();
    let session = created["session"]["id"].as_str().unwrap().to_owned();
    let released = dispatch(&st, "project.release", json!({"projectId":project,"agentId":agent})).await.unwrap();
    assert_eq!(released["released"], true);
    assert!(st.db.project_members(&project).unwrap().is_empty());
    let after = st.db.agent_get(&agent).unwrap();
    assert_eq!(after["projectId"], Value::Null, "a default pointing at the project is cleared");
    assert!(st.db.session_exists(&session).unwrap(), "the conversation stays");
    assert_eq!(dispatch(&st, "agent.conversations", json!({"agentId":agent})).await.unwrap()["conversations"].as_array().unwrap().len(), 1);
    assert_eq!(dispatch(&st, "project.release", json!({"projectId":project,"agentId":agent})).await.unwrap()["released"], false);
}

#[tokio::test]
async fn a_new_conversation_in_a_project_hires_its_bot() {
    let (st, _events) = state();
    let project = a_project(&st, "Fresh");
    let agent = st.db.agent_create(&json!({"name":"Ada","runtimeId":"rt-test"})).unwrap().0["id"].as_str().unwrap().to_owned();
    assert!(st.db.project_members(&project).unwrap().is_empty());
    let created = dispatch(&st, "agent.conversation.create", json!({"agentId":agent,"projectId":project})).await.unwrap();
    let _session = created["session"]["id"].as_str().unwrap().to_owned();
    let members = st.db.project_members(&project).unwrap();
    assert_eq!(members.len(), 1);
    assert_eq!(members[0]["agentId"], agent.as_str());
    let listed = dispatch(&st, "agent.projects", json!({"agentId":agent})).await.unwrap()["projects"].clone();
    let row = listed.as_array().unwrap().iter().find(|row| row["id"] == project.as_str()).cloned().unwrap();
    assert_eq!(row["conversationCount"], 1);
    // A conversation in a project hires a bot that had no default project too.
    let other = a_project(&st, "Second");
    dispatch(&st, "agent.conversation.create", json!({"agentId":agent,"projectId":other})).await.unwrap();
    assert_eq!(st.db.project_members(&other).unwrap().len(), 1);
    // The CEO belongs to every project implicitly: a conversation never hires it.
    let boss = dispatch(&st, "manager.ensure", json!({"runtimeId":"rt-test"})).await.unwrap()["manager"].clone();
    let boss_id = boss["id"].as_str().unwrap().to_owned();
    dispatch(&st, "agent.conversation.create", json!({"agentId":boss_id,"projectId":project})).await.unwrap();
    assert_eq!(st.db.project_members(&project).unwrap().len(), 1, "the CEO gets no row");
}

#[tokio::test]
async fn preset_list_reports_the_employee_each_preset_made() {
    let (st, _events) = state();
    let project = a_project(&st, "Catalogue");
    let listed = dispatch(&st, "preset.list", json!({})).await.unwrap();
    assert_eq!(listed["departments"].as_array().unwrap().len(), 7);
    let presets = listed["presets"].as_array().unwrap();
    assert_eq!(presets.len(), 28);
    assert!(presets.iter().all(|preset| preset["hiredAgentId"].is_null()), "nothing is hired on a fresh install");
    let hired = dispatch(&st, "project.hire", json!({"projectId":project,"presetId":"ada"})).await.unwrap();
    let after = dispatch(&st, "preset.list", json!({})).await.unwrap();
    let presets = after["presets"].as_array().unwrap();
    let ada = presets.iter().find(|preset| preset["id"] == "ada").cloned().unwrap();
    assert_eq!(ada["hiredAgentId"], hired["agentId"]);
    assert_eq!(ada["nickname"], "Ada");
    assert_eq!(ada["portrait"], "char-ada-lovelace");
    assert_eq!(ada["photo"], "photo-17");
    assert_eq!(ada["department"], "engineering");
    assert_eq!(ada["role"], "Backend engineer");
    assert_eq!(ada["color"], "violet");
    assert!(ada["tags"].as_array().unwrap().contains(&json!("backend")));
    assert!(presets.iter().filter(|preset| preset["hiredAgentId"].is_null()).count() == 27);
}

#[tokio::test]
async fn the_ceo_proposes_hires_as_one_group_and_approval_hires() {
    let (st, _events) = state();
    let project = a_project(&st, "Team");
    let boss = dispatch(&st, "manager.ensure", json!({"runtimeId":"rt-test"})).await.unwrap()["manager"].clone();
    let boss_id = boss["id"].as_str().unwrap().to_owned();
    st.db.create_session_for_agent("ceo-main", "rt-test", "codex", ".", "CEO", Some(&boss_id)).unwrap();
    st.db.update_session_state("ceo-main", "idle").unwrap();
    // The two new tools sit next to propose_bot for the CEO only.
    let tools = tool_names(&team::mcp_rpc(&st, "ceo-main", "mcp.tools.list", &json!({})).await.unwrap());
    for tool in ["list_presets", "propose_hires"] { assert!(tools.contains(&tool.to_owned()), "{tool} is offered to the CEO"); }
    // One candidate is already in the project: it is named and skipped.
    let ada = dispatch(&st, "project.hire", json!({"projectId":project,"presetId":"ada"})).await.unwrap();
    let solo = st.db.agent_create(&json!({"name":"Solo","runtimeId":"rt-test"})).unwrap().0["id"].as_str().unwrap().to_owned();
    let text = call_tool(&st, "ceo-main", "propose_hires", json!({"project":project,"candidates":[
        {"preset":"ada","reason":"Already working here"},
        {"preset":"wolfie","reason":"The interface needs building"},
        {"bot":"Solo","reason":"Already on the payroll"}
    ]})).await;
    assert_ne!(text["isError"], true, "{text}");
    let body = text["content"][0]["text"].as_str().unwrap();
    assert!(body.contains("Skipped") && body.contains("Ada is already in Team"), "{body}");
    assert!(body.contains("Hire Wolfie into Team"), "{body}");
    let pending = st.db.proposals_pending().unwrap();
    assert_eq!(pending.len(), 2, "one proposal per candidate that was not skipped");
    assert!(pending.iter().all(|row| row["kind"] == "hire"));
    let group = pending[0]["payload"]["groupId"].as_str().unwrap().to_owned();
    assert_eq!(pending[1]["payload"]["groupId"], group.as_str(), "one card group per call");
    let wolfie = pending.iter().find(|row| row["payload"]["presetId"] == "wolfie").cloned().expect("the preset candidate");
    let named = pending.iter().find(|row| row["payload"]["agentId"].as_str() == Some(solo.as_str())).cloned().expect("the existing employee");
    assert_eq!(wolfie["payload"]["projectId"], project.as_str());
    assert_eq!(wolfie["payload"]["projectName"], "Team");
    assert_eq!(wolfie["payload"]["nickname"], "Wolfie");
    assert_eq!(wolfie["payload"]["role"], "Frontend engineer");
    assert_eq!(wolfie["payload"]["portrait"], "char-wolfgang-amadeus-mozart");
    assert_eq!(wolfie["payload"]["photo"], "photo-31");
    assert_eq!(wolfie["payload"]["reason"], "The interface needs building");
    assert_eq!(named["payload"]["nickname"], "Solo");
    // The card's own title and detail.
    let card = st.db.permission_get(wolfie["id"].as_str().unwrap()).unwrap();
    assert_eq!(card["title"], "Hire Wolfie into Team");
    assert_eq!(card["detail"], "Frontend engineer · The interface needs building");
    // Approval hires, with the owner's edits for a runtime.
    let decided = dispatch(&st, "proposal.decide", json!({"id":wolfie["id"],"decision":"approve","edits":{"runtimeId":"rt-test"}})).await.unwrap();
    assert_eq!(decided["result"]["created"], true);
    assert_eq!(st.db.project_members(&project).unwrap().len(), 2, "Ada plus the hired Wolfie");
    // Rejecting changes nothing.
    let before = st.db.project_members(&project).unwrap();
    dispatch(&st, "proposal.decide", json!({"id":named["id"],"decision":"reject"})).await.unwrap();
    assert_eq!(st.db.project_members(&project).unwrap(), before);
    let _ = ada;
    // A candidate needs a preset or a bot, not both.
    let refused = call_tool(&st, "ceo-main", "propose_hires", json!({"project":project,"candidates":[{"preset":"ada","bot":"Solo","reason":"Both"}]})).await;
    assert_eq!(refused["isError"], true);
}

#[tokio::test]
async fn list_presets_filters_and_is_read_only() {
    let (st, _events) = state();
    let boss = dispatch(&st, "manager.ensure", json!({"runtimeId":"rt-test"})).await.unwrap()["manager"].clone();
    let boss_id = boss["id"].as_str().unwrap().to_owned();
    st.db.create_session_for_agent("ceo-list", "rt-test", "codex", ".", "CEO", Some(&boss_id)).unwrap();
    st.db.update_session_state("ceo-list", "idle").unwrap();
    let all = call_tool(&st, "ceo-list", "list_presets", json!({})).await;
    let body = all["content"][0]["text"].as_str().unwrap();
    assert!(body.contains("Departments:") && body.contains("Engineering (engineering)"), "{body}");
    assert_eq!(body.lines().filter(|line| line.starts_with("- ")).count(), 28);
    let data = call_tool(&st, "ceo-list", "list_presets", json!({"department":"data"})).await;
    let data = data["content"][0]["text"].as_str().unwrap();
    assert_eq!(data.lines().filter(|line| line.starts_with("- ")).count(), 3, "{data}");
    assert!(!data.contains("char-ada-lovelace"));
    let tagged = call_tool(&st, "ceo-list", "list_presets", json!({"tag":"security"})).await;
    let tagged = tagged["content"][0]["text"].as_str().unwrap();
    assert!(tagged.contains("Jo") && !tagged.contains("Ada"), "{tagged}");
    let none = call_tool(&st, "ceo-list", "list_presets", json!({"department":"nope"})).await;
    assert!(none["content"][0]["text"].as_str().unwrap().contains("No preset employee matches"));
    assert!(st.db.proposals_pending().unwrap().is_empty(), "the read tools create nothing");
}

// A stock profile photo belongs to one employee (9 November 2026).
#[tokio::test]
async fn a_stock_photo_is_worn_by_one_employee_and_a_preset_falls_back_to_the_silhouette() {
    let (st, _events) = state();
    let look = |photo: &str| json!({"shape":"circle","photo":photo});
    let first = dispatch(&st, "agent.create", json!({"name":"One","runtimeId":"rt-test","look":look("photo-05")})).await.unwrap()["agent"].clone();
    let first_id = first["id"].as_str().unwrap().to_owned();
    let taken = dispatch(&st, "agent.create", json!({"name":"Two","runtimeId":"rt-test","look":look("photo-05")})).await.unwrap_err();
    assert_eq!(taken.1.code, "conflict");
    assert_eq!(taken.1.message, "Another employee already wears this photo. Change theirs first.");
    // Another photo and an own upload are free; uploads are not checked.
    let second = dispatch(&st, "agent.create", json!({"name":"Two","runtimeId":"rt-test","look":look("photo-06")})).await.unwrap()["agent"].clone();
    let second_id = second["id"].as_str().unwrap().to_owned();
    let upload = "data:image/webp;base64,UklGRg==";
    for name in ["Three", "Four"] { dispatch(&st, "agent.create", json!({"name":name,"runtimeId":"rt-test","look":look(upload)})).await.unwrap(); }
    // An employee may save its own photo again, but not take another's.
    dispatch(&st, "agent.update", json!({"agentId":first_id,"look":look("photo-05")})).await.unwrap();
    let steal = dispatch(&st, "agent.update", json!({"agentId":second_id,"look":look("photo-05")})).await.unwrap_err();
    assert_eq!(steal.1.code, "conflict");
    // An archived employee still wears its photo.
    dispatch(&st, "agent.delete", json!({"agentId":first_id})).await.unwrap();
    assert_eq!(dispatch(&st, "agent.update", json!({"agentId":second_id,"look":look("photo-05")})).await.unwrap_err().1.code, "conflict");
    // Hiring a preset whose photo is taken succeeds with no photo.
    dispatch(&st, "agent.create", json!({"name":"Holder","runtimeId":"rt-test","look":look("photo-17")})).await.unwrap();
    let project = a_project(&st, "P");
    let hired = dispatch(&st, "project.hire", json!({"projectId":project,"presetId":"ada"})).await.unwrap();
    let ada = st.db.agent_get(hired["agentId"].as_str().unwrap()).unwrap();
    assert_eq!(ada["name"], "Ada");
    assert!(ada["look"]["photo"].is_null(), "{}", ada["look"]);
}

// Coding agents the owner turned off (9 November 2026). Since 11 November 2026 a
// main provider cannot be turned off by its provider id; one installed copy is
// still switched off by its runtime id.
#[tokio::test]
async fn a_turned_off_coding_agent_is_marked_kept_and_refused() {
    let (st, _events) = state();
    for bad in [json!("codex"), json!([1]), json!([""]), json!(vec!["x"; 33]), json!(["bad id"])] {
        assert_eq!(dispatch(&st, "settings.set", json!({"key":"runtimes.disabled","value":bad})).await.unwrap_err().1.code, "invalid_argument", "{bad}");
    }
    assert!(disabled_runtimes(&st.db).is_empty());
    // Codex is a main provider: its own id is refused, with the sentence the UI
    // shows, and nothing is switched off.
    let main = dispatch(&st, "settings.set", json!({"key":"runtimes.disabled","value":["codex"]})).await.unwrap_err();
    assert_eq!(main.1.code, "invalid_argument");
    assert_eq!(main.1.message, "Codex is always on and cannot be turned off. Turn off one installed copy instead.");
    assert!(disabled_runtimes(&st.db).is_empty());
    dispatch(&st, "settings.set", json!({"key":"runtimes.disabled","value":["rt-test"]})).await.unwrap();
    assert_eq!(disabled_runtimes(&st.db), vec!["rt-test".to_owned()]);
    // The saved row is kept, published, and marked off.
    let roster = st.runtimes.read().await.clone();
    let row = roster.iter().find(|row| row["id"] == "rt-test").expect("the saved row is still published");
    assert_eq!(row["disabled"], true, "{roster:?}");
    assert_eq!(row["status"], "disabled");
    assert_eq!(row["main"], true, "Codex is a main provider");
    assert_eq!(row["enabled"], false, "this installed copy is off");
    assert!(!catalog::runtime_usable(row));
    assert!(st.db.runtimes().unwrap().iter().any(|row| row["id"] == "rt-test" && row["disabled"] == true));
    // A conversation on it is refused with the plain sentence.
    let refused = dispatch(&st, "session.new", json!({"projectPath":".","runtimeId":"rt-test"})).await.unwrap_err();
    assert_eq!(refused.1.message, "This coding agent is turned off in Settings \u{203a} Coding agents.");
    // Turned back on: the mark clears.
    dispatch(&st, "settings.set", json!({"key":"runtimes.disabled","value":[]})).await.unwrap();
    let mut row = st.db.runtimes().unwrap().into_iter().find(|row| row["id"] == "rt-test").unwrap();
    stamp_runtime_state(&mut row, &[]);
    assert!(row.get("disabled").is_none());
    assert_eq!(row["enabled"], true);
    // A single installed copy can be turned off by its runtime id.
    assert!(runtime_is_disabled(&json!({"id":"rt-test","provider":"codex"}), &["rt-test".to_owned()]));
    assert!(!runtime_is_disabled(&json!({"id":"rt-other","provider":"codex"}), &["rt-test".to_owned()]));
}
