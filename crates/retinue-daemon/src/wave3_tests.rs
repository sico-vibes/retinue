// Wave 3, learned skills and first-run suggestions (nod ideas 1 and 5). Included
// into the daemon's test module, so `state`, `dispatch`, `call_tool`,
// `memory_agent`, `memory_session`, `skills_state`, `tool_names` and
// `next_event` are in scope. Every test here calls the RPCs through `dispatch`
// and the bot tools through the team bridge, because a method the dispatch
// cannot reach is the wave 2 mistake this lane is asked not to repeat.

/// The text of an MCP tool reply.
fn reply_text(reply: &Value) -> String {
    reply["content"][0]["text"].as_str().unwrap_or("").to_owned()
}

/// A running turn the owner started, which is what `save_skill` requires.
async fn owner_turn(st: &AppState, sid: &str, turn: &str) {
    st.active_turns.lock().await.insert(sid.to_owned(), turn.to_owned());
    crate::skills::note_turn_origin(sid, turn, true);
}

fn saved_skill(name: &str, description: &str, body: &str) -> Value {
    json!({"name": name, "description": description, "body": body})
}

#[tokio::test]
async fn a_bot_saves_a_skill_and_the_owner_reviews_it_through_the_rpc() {
    let (st, mut events, data) = skills_state();
    let writer = memory_agent(&st, "Learner");
    let sid = "learn-session";
    memory_session(&st, &writer, sid);

    // No turn at all, then a turn the owner did not start: both refused.
    let no_turn = call_tool(&st, sid, "save_skill", saved_skill("release-gates", "How to check a release.", "Run the tests, then check the changelog.")).await;
    assert_eq!(no_turn["isError"], true, "{no_turn}");
    assert!(reply_text(&no_turn).contains("Only a turn the owner started"), "{no_turn}");
    st.active_turns.lock().await.insert(sid.to_owned(), "turn-1".to_owned());
    let teammate = call_tool(&st, sid, "save_skill", saved_skill("release-gates", "How to check a release.", "Run the tests, then check the changelog.")).await;
    assert_eq!(teammate["isError"], true, "{teammate}");

    crate::skills::note_turn_origin(sid, "turn-1", true);
    let saved = call_tool(&st, sid, "save_skill", saved_skill("release-gates", "How to check a release.", "1. Run the tests.\n2. Check the changelog.")).await;
    assert!(saved.get("isError").is_none(), "{saved}");
    assert!(reply_text(&saved).contains("Saved the skill \"release-gates\""), "{saved}");
    let event = next_event(&mut events, "skills.learned.changed").await;
    assert_eq!(event.payload["action"], "saved");
    assert_eq!(event.payload["name"], "release-gates");
    assert_eq!(event.payload["agentId"], writer.as_str());
    // The file is a real skill folder, so loading it works like any other.
    assert!(data.join("skills").join("release-gates").join("SKILL.md").is_file());

    // The review queue names the bot that learned it and carries no body.
    let queue = dispatch(&st, "skills.learned.list", json!({"reviewState":"pending"})).await.unwrap();
    let rows = queue["skills"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{queue}");
    let row = rows[0].clone();
    assert_eq!(row["name"], "release-gates");
    assert_eq!(row["source"], "agent");
    assert_eq!(row["reviewState"], "pending");
    assert_eq!(row["learnedBy"], writer.as_str());
    assert_eq!(row["learnedByName"], "Learner");
    assert_eq!(row["ownerEdited"], false);
    assert!(row.get("body").is_none(), "the queue carries no body");
    assert_eq!(dispatch(&st, "skills.learned.list", json!({"reviewState":"approved"})).await.unwrap()["skills"].as_array().unwrap().len(), 0);
    assert_eq!(dispatch(&st, "skills.learned.list", json!({"reviewState":"nonsense"})).await.unwrap_err().1.code, "invalid_argument");

    // The body is read through the ordinary skill read, which the Skills page
    // already uses.
    let got = dispatch(&st, "skills.get", json!({"name":"release-gates"})).await.unwrap();
    assert!(got["body"].as_str().unwrap().contains("1. Run the tests."), "{got}");

    // Approval joins it to the pool, and the event says so.
    let approved = dispatch(&st, "skills.learned.approve", json!({"name":"release-gates"})).await.unwrap();
    assert_eq!(approved["skill"]["reviewState"], "approved");
    assert_eq!(next_event(&mut events, "skills.learned.changed").await.payload["action"], "approved");

    // The owner's edit locks the bot out of patching it.
    let edited = dispatch(&st, "skills.learned.edit", json!({"name":"release-gates","body":"Run every gate before merging."})).await.unwrap();
    assert_eq!(edited["skill"]["ownerEdited"], true);
    assert_eq!(edited["skill"]["reviewState"], "approved");
    assert!(edited["skill"]["description"].as_str().unwrap().contains("How to check a release"), "an edit keeps the description it was not given");
    assert_eq!(next_event(&mut events, "skills.learned.changed").await.payload["action"], "edited");
    let refused = call_tool(&st, sid, "patch_skill", json!({"name":"release-gates","body":"Something else."})).await;
    assert_eq!(refused["isError"], true, "{refused}");
    assert!(reply_text(&refused).contains("The owner edited"), "{refused}");
    let refused = call_tool(&st, sid, "save_skill", saved_skill("release-gates", "How to check a release.", "Overwrite the owner.")).await;
    assert_eq!(refused["isError"], true, "a save cannot overwrite an owner-edited skill either: {refused}");
    let body = dispatch(&st, "skills.get", json!({"name":"release-gates"})).await.unwrap()["body"].as_str().unwrap().to_owned();
    assert!(body.contains("Run every gate before merging."), "the owner's text stands: {body}");

    // Forgetting needs confirmation, removes the row and the folder.
    assert_eq!(dispatch(&st, "skills.learned.forget", json!({"name":"release-gates"})).await.unwrap_err().1.code, "invalid_argument");
    dispatch(&st, "skills.learned.forget", json!({"name":"release-gates","confirm":true})).await.unwrap();
    assert_eq!(next_event(&mut events, "skills.learned.changed").await.payload["action"], "forgotten");
    assert!(st.db.skills_get("release-gates").is_err());
    assert!(!data.join("skills").join("release-gates").exists());
    assert_eq!(dispatch(&st, "skills.learned.forget", json!({"name":"release-gates","confirm":true})).await.unwrap_err().1.code, "not_found");
    let _ = std::fs::remove_dir_all(&data);
}

#[tokio::test]
async fn a_learned_draft_is_redacted_and_reserved_names_are_refused() {
    let (st, _events, data) = skills_state();
    let writer = memory_agent(&st, "Redactor");
    let sid = "redact-session";
    memory_session(&st, &writer, sid);
    owner_turn(&st, sid, "turn-redact").await;

    // A secret, a card number and a sign-in link are refused outright.
    let secret = format!("sk-ant-{}", "abcdefghijklmnopqrstuvwxyz012345");
    for body in [format!("Use the key {secret} to call the API."), "Card 4111 1111 1111 1234 was charged.".to_owned(), "Sign in at https://example.com/login?next=home first.".to_owned()] {
        let refused = call_tool(&st, sid, "save_skill", saved_skill("draft-one", "A draft.", &body)).await;
        assert_eq!(refused["isError"], true, "{body}: {refused}");
        assert!(reply_text(&refused).contains("credential, a card number or a sign-in link"), "{refused}");
        assert!(st.db.skills_get("draft-one").is_err(), "nothing is stored");
    }

    // An ordinary draft keeps its steps and loses the person, the address and
    // the long number.
    let saved = call_tool(&st, sid, "save_skill", saved_skill(
        "invoice-steps",
        "How invoices are checked.",
        "Ask Ann at ann@example.com for the total.\nThe account is 123456789.\nThe guide is at https://example.com/invoices.\nThe checkout lives in C:\\Users\\Ann\\dev\\app.",
    )).await;
    assert!(saved.get("isError").is_none(), "{saved}");
    let body = dispatch(&st, "skills.get", json!({"name":"invoice-steps"})).await.unwrap()["body"].as_str().unwrap().to_owned();
    assert!(body.contains("<email>"), "{body}");
    assert!(body.contains("<number>"), "{body}");
    assert!(body.contains("<link>"), "{body}");
    assert!(body.contains("C:\\Users\\<user>\\dev\\app"), "{body}");
    assert!(!body.contains("Ann@") && !body.contains("ann@"), "no address survives: {body}");
    assert!(!body.contains("123456789"), "no long number survives: {body}");
    assert!(body.contains("Ask Ann at"), "the steps stay readable: {body}");

    // Built-in and vendored names are reserved.
    let builtin = call_tool(&st, sid, "save_skill", saved_skill("retinue", "Mine now.", "Body.")).await;
    assert_eq!(builtin["isError"], true, "{builtin}");
    assert!(reply_text(&builtin).contains("built-in skill name"), "{builtin}");
    let vendored = call_tool(&st, sid, "save_skill", saved_skill("api-design", "Mine now.", "Body.")).await;
    assert_eq!(vendored["isError"], true, "{vendored}");
    assert!(reply_text(&vendored).contains("Retinue ships"), "{vendored}");

    // A bad name, an empty body and an over-long body.
    let bad_name = call_tool(&st, sid, "save_skill", saved_skill("A Bad Name", "Draft.", "Body.")).await;
    assert!(reply_text(&bad_name).contains("lower case letters"), "{bad_name}");
    let empty = call_tool(&st, sid, "save_skill", saved_skill("empty-one", "Draft.", "   ")).await;
    assert!(reply_text(&empty).contains("A skill body must be 1 to"), "{empty}");
    let long = call_tool(&st, sid, "save_skill", saved_skill("long-one", "Draft.", &"x".repeat(crate::skills::MAX_LEARNED_BODY_CHARS + 1))).await;
    assert!(reply_text(&long).contains("A skill body must be 1 to"), "{long}");
    // A shared skill another bot imported is a conflict, never a silent replace.
    std::fs::create_dir_all(data.join("imported")).unwrap();
    st.db.skills_upsert(&json!({"name":"imported","description":"Imported.","source":"folder","contentSha256":"a".repeat(64),"fileCount":1,"totalBytes":10})).unwrap();
    let conflict = call_tool(&st, sid, "save_skill", saved_skill("imported", "Mine.", "Body.")).await;
    assert!(reply_text(&conflict).contains("already exists"), "{conflict}");
    // A patch only reaches the bot's own learned skill.
    let foreign = call_tool(&st, sid, "patch_skill", json!({"name":"imported","body":"Body."})).await;
    assert!(reply_text(&foreign).contains("not a skill this bot saved"), "{foreign}");
    let _ = std::fs::remove_dir_all(&data);
}

#[tokio::test]
async fn a_learned_skill_is_private_to_its_bot_until_it_is_approved() {
    let (st, _events, data) = skills_state();
    let writer = memory_agent(&st, "Keeper");
    let reader = memory_agent(&st, "Other");
    let mine = "keeper-session";
    let theirs = "other-session";
    memory_session(&st, &writer, mine);
    memory_session(&st, &reader, theirs);
    owner_turn(&st, mine, "turn-keeper").await;

    let saved = call_tool(&st, mine, "save_skill", saved_skill("nightly-job", "How the nightly job is run.", "Start it after the backups.")).await;
    assert!(saved.get("isError").is_none(), "{saved}");

    // The learner sees it in its list and can load it; another bot sees neither.
    let mine_list = reply_text(&call_tool(&st, mine, "list_skills", json!({})).await);
    assert!(mine_list.contains("nightly-job"), "{mine_list}");
    let theirs_list = reply_text(&call_tool(&st, theirs, "list_skills", json!({})).await);
    assert!(!theirs_list.contains("nightly-job"), "{theirs_list}");
    let loaded = call_tool(&st, mine, "load_skill", json!({"name":"nightly-job"})).await;
    assert!(loaded.get("isError").is_none(), "{loaded}");
    assert!(reply_text(&loaded).contains("Start it after the backups."), "{loaded}");
    let denied = call_tool(&st, theirs, "load_skill", json!({"name":"nightly-job"})).await;
    assert_eq!(denied["isError"], true, "{denied}");
    assert!(reply_text(&denied).contains("not available to you"), "{denied}");
    // The per-bot view the composer asks for follows the same rule.
    assert!(dispatch(&st, "skills.forAgent", json!({"agentId":reader})).await.unwrap()["skills"].as_array().unwrap().iter().all(|row| row["name"] != "nightly-job"));
    assert!(dispatch(&st, "skills.forAgent", json!({"agentId":writer})).await.unwrap()["skills"].as_array().unwrap().iter().any(|row| row["name"] == "nightly-job"));

    // Approval releases it to every bot.
    dispatch(&st, "skills.learned.approve", json!({"name":"nightly-job"})).await.unwrap();
    let theirs_list = reply_text(&call_tool(&st, theirs, "list_skills", json!({})).await);
    assert!(theirs_list.contains("nightly-job"), "{theirs_list}");
    assert!(call_tool(&st, theirs, "load_skill", json!({"name":"nightly-job"})).await.get("isError").is_none());
    let _ = std::fs::remove_dir_all(&data);
}

#[tokio::test]
async fn automation_runs_get_the_read_tools_only_and_cannot_save() {
    let (st, _events, data) = skills_state();
    let agent = memory_agent(&st, "Scheduled");
    let auto = "auto-session";
    memory_session(&st, &agent, auto);
    st.db.set_session_link(auto, &json!({"kind":"automation","automationId":"a1","runId":"r1","approvalMode":"auto"})).unwrap();
    st.active_turns.lock().await.insert(auto.to_owned(), "auto-turn".to_owned());
    crate::skills::note_turn_origin(auto, "auto-turn", true);

    let names = tool_names(&team::mcp_rpc(&st, auto, "mcp.tools.list", &json!({})).await.unwrap());
    for read in ["list_skills", "load_skill", "read_skill_file"] {
        assert!(names.contains(&read.to_owned()), "an automation run keeps the read tools: {names:?}");
    }
    for write in ["save_skill", "patch_skill"] {
        assert!(!names.contains(&write.to_owned()), "an automation run is never offered {write}: {names:?}");
    }
    // Even a hand-written call is refused, and the owner's turn does not help.
    // It is refused at the RPC, before the tool arm: a name the automation
    // allowlist does not carry never reaches the tool at all.
    let refused = team::mcp_rpc(&st, auto, "mcp.tools.call", &json!({"name":"save_skill","arguments":saved_skill("auto-skill", "Learned on a schedule.", "Body.")})).await.unwrap_err();
    assert_eq!(refused.1.code, "forbidden", "{refused:?}");
    assert!(refused.1.message.contains("Automation runs cannot use team or scheduling tools"), "{refused:?}");
    assert!(st.db.skills_get("auto-skill").is_err());
    // An attended bot is offered both write tools.
    let human = memory_agent(&st, "Attended");
    let sid = "attended-session";
    memory_session(&st, &human, sid);
    let names = tool_names(&team::mcp_rpc(&st, sid, "mcp.tools.list", &json!({})).await.unwrap());
    assert!(names.contains(&"save_skill".to_owned()) && names.contains(&"patch_skill".to_owned()), "{names:?}");
    let _ = std::fs::remove_dir_all(&data);
}

#[tokio::test]
async fn the_cap_evicts_the_least_recently_used_and_owner_edits_stop_the_eviction() {
    let (st, mut events, data) = skills_state();
    let writer = memory_agent(&st, "Collector");
    let sid = "collector-session";
    memory_session(&st, &writer, sid);
    owner_turn(&st, sid, "turn-collect").await;
    // Forty learned skills, written straight through storage: the tool path is
    // covered by the other tests, this one is about the cap.
    for index in 0..crate::skills::LEARNED_CAP {
        st.db
            .skills_learned_upsert(&json!({
                "name": format!("kept-{index}"),
                "description": "A kept skill.",
                "contentSha256": "b".repeat(64),
                "fileCount": 1,
                "totalBytes": 10,
                "learnedBy": writer,
            }))
            .unwrap();
    }
    // The first and the last are used; the second is the least recently used.
    // The pause makes the load stamps later than every write, so the order does
    // not depend on two rows sharing a millisecond.
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    st.db.skills_learned_touch("kept-0").unwrap();
    st.db.skills_learned_touch("kept-39").unwrap();
    let saved = call_tool(&st, sid, "save_skill", saved_skill("one-more", "One more.", "Body of the new one.")).await;
    assert!(saved.get("isError").is_none(), "{saved}");
    assert!(st.db.skills_get("kept-1").is_err(), "the least recently used row was evicted");
    assert!(st.db.skills_get("kept-0").is_ok() && st.db.skills_get("kept-39").is_ok(), "the used rows stay");
    assert!(st.db.skills_get("one-more").is_ok());
    assert!(!data.join("skills").join("kept-1").exists(), "an evicted skill leaves no folder");
    // The eviction is announced before the save, so the window can drop the row
    // and add the new one in the order they happened.
    let evicted = next_event(&mut events, "skills.learned.changed").await;
    assert_eq!(evicted.payload["action"], "evicted");
    assert_eq!(evicted.payload["name"], "kept-1");
    assert_eq!(next_event(&mut events, "skills.learned.changed").await.payload["action"], "saved");

    // With every remaining row owner-edited, the cap is a refusal, not a loss.
    let rows = st.db.skills_learned_list(Some(&writer)).unwrap();
    for row in rows {
        let name = row["name"].as_str().unwrap().to_owned();
        st.db.skills_learned_owner_edit(&name, "Edited.", &"c".repeat(64), 10).unwrap();
    }
    let refused = call_tool(&st, sid, "save_skill", saved_skill("too-many", "One too many.", "Body.")).await;
    assert_eq!(refused["isError"], true, "{refused}");
    assert!(reply_text(&refused).contains("the owner edited them all"), "{refused}");
    let _ = std::fs::remove_dir_all(&data);
}

#[tokio::test]
async fn first_run_suggestions_come_from_the_project_folder() {
    let (st, _events) = state();
    let folder = std::env::temp_dir().join(format!("retinue-suggest-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("README.md"), "# Invoice parser\n\nA small parser.\n").unwrap();
    std::fs::write(folder.join("package.json"), "{\"name\":\"invoices\",\"scripts\":{\"build\":\"tsc\",\"test\":\"vitest run\"}}").unwrap();
    std::fs::write(folder.join("AGENTS.md"), "# Rules for agents\n\nNever commit secrets.\n").unwrap();
    // A real repository, when git is on this machine: the commit suggestion
    // needs one. Without git the reply simply has two grounded suggestions.
    let has_git = std::process::Command::new("git").arg("--version").output().map(|output| output.status.success()).unwrap_or(false);
    if has_git {
        let git = |args: &[&str]| std::process::Command::new("git").current_dir(&folder).args(args).output().map(|output| output.status.success()).unwrap_or(false);
        git(&["init", "-q"]);
        git(&["config", "core.autocrlf", "false"]);
        std::fs::write(folder.join("parser.ts"), "export const parse = () => 1\n").unwrap();
        git(&["add", "."]);
        git(&["-c", "user.name=Test", "-c", "user.email=test@retinue.local", "commit", "-q", "-m", "Set up the invoice parser"]);
    }
    let project = st.db.project_create("Invoices", Some(&folder.to_string_lossy())).unwrap().0;
    let id = project["id"].as_str().unwrap().to_owned();
    let reply = dispatch(&st, "project.suggestions", json!({"projectId":id})).await.unwrap();
    let suggestions = reply["suggestions"].as_array().unwrap();
    assert_eq!(suggestions.len(), 3, "always three cards: {reply}");
    assert_eq!(reply["projectId"], id.as_str());
    assert_eq!(reply["degraded"], !has_git, "degraded only when a card had to be static: {reply}");
    assert_eq!(reply["folder"].as_str().unwrap(), folder.to_string_lossy(), "the reply names the folder it read");

    let readme = &suggestions[0];
    assert_eq!(readme["kind"], "readme");
    assert_eq!(readme["source"]["file"], "README.md");
    assert_eq!(readme["source"]["detail"], "heading: Invoice parser");
    assert!(readme["prompt"].as_str().unwrap().contains("Invoice parser"), "{readme}");

    let script = &suggestions[1];
    assert_eq!(script["kind"], "script");
    assert_eq!(script["source"]["file"], "package.json");
    assert_eq!(script["source"]["detail"], "script: test", "the test script is preferred");
    assert!(script["prompt"].as_str().unwrap().contains("npm run test"), "{script}");

    let Some(third) = suggestions.iter().find(|suggestion| suggestion["kind"] != "readme" && suggestion["kind"] != "script") else { panic!("a third card: {reply}") };
    if has_git {
        assert_eq!(third["kind"], "commit", "{reply}");
        assert!(third["source"]["detail"].as_str().unwrap().contains("Set up the invoice parser"), "{third}");
        assert!(third["prompt"].as_str().unwrap().contains("Set up the invoice parser"), "{third}");
    }
    // AGENTS.md grounds the fourth kind, used when one of the others is missing.
    let only_rules = std::env::temp_dir().join(format!("retinue-suggest-rules-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&only_rules).unwrap();
    std::fs::write(only_rules.join("AGENTS.md"), "# Rules for agents\n\nNever commit secrets.\n").unwrap();
    let project = st.db.project_create("Rules only", Some(&only_rules.to_string_lossy())).unwrap().0;
    let reply = dispatch(&st, "project.suggestions", json!({"projectId":project["id"].as_str().unwrap()})).await.unwrap();
    let agents = reply["suggestions"].as_array().unwrap().iter().find(|row| row["kind"] == "agents").cloned().expect("an AGENTS.md suggestion");
    assert_eq!(agents["source"]["file"], "AGENTS.md");
    assert_eq!(agents["source"]["detail"], "heading: Rules for agents");

    // The rails: no project, no folder, an unreadable file.
    assert_eq!(dispatch(&st, "project.suggestions", json!({"projectId":"nope"})).await.unwrap_err().1.code, "not_found");
    assert_eq!(dispatch(&st, "project.suggestions", json!({})).await.unwrap_err().1.code, "invalid_argument");
    let big = std::env::temp_dir().join(format!("retinue-suggest-big-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&big).unwrap();
    std::fs::write(big.join("README.md"), format!("# Huge\n{}", "x".repeat(70 * 1024))).unwrap();
    let project = st.db.project_create("Big", Some(&big.to_string_lossy())).unwrap().0;
    let reply = dispatch(&st, "project.suggestions", json!({"projectId":project["id"].as_str().unwrap()})).await.unwrap();
    assert!(reply["suggestions"].as_array().unwrap().iter().all(|row| row["source"] != json!({"file":"README.md","detail":"heading: Huge"})), "a file past the cap is not read: {reply}");
    let _ = std::fs::remove_dir_all(&folder);
    let _ = std::fs::remove_dir_all(&only_rules);
    let _ = std::fs::remove_dir_all(&big);
}

#[tokio::test]
async fn first_run_suggestions_degrade_to_three_static_prompts() {
    let (st, _events) = state();
    let project = st.db.project_create("Nowhere", None).unwrap().0;
    let reply = dispatch(&st, "project.suggestions", json!({"projectId":project["id"].as_str().unwrap()})).await.unwrap();
    assert_eq!(reply["degraded"], true);
    assert!(reply["folder"].is_null());
    let suggestions = reply["suggestions"].as_array().unwrap();
    assert_eq!(suggestions.len(), 3);
    for (index, suggestion) in suggestions.iter().enumerate() {
        assert_eq!(suggestion["kind"], "static");
        assert_eq!(suggestion["id"], format!("static-{}", index + 1));
        assert!(suggestion["source"].is_null());
        assert!(!suggestion["prompt"].as_str().unwrap().is_empty());
    }
    // A folder that no longer exists reads as no folder at all.
    let missing = std::env::temp_dir().join(format!("retinue-suggest-gone-{}", Uuid::new_v4()));
    let project = st.db.project_create("Gone", Some(&missing.to_string_lossy())).unwrap().0;
    let reply = dispatch(&st, "project.suggestions", json!({"projectId":project["id"].as_str().unwrap()})).await.unwrap();
    assert_eq!(reply["degraded"], true);
    assert_eq!(reply["suggestions"][0]["kind"], "static");
}

#[tokio::test]
async fn a_purged_bot_takes_its_learned_skills_with_it() {
    let (st, _events, data) = skills_state();
    let writer = memory_agent(&st, "Leaving");
    let sid = "leaving-session";
    memory_session(&st, &writer, sid);
    owner_turn(&st, sid, "turn-leaving").await;
    let saved = call_tool(&st, sid, "save_skill", saved_skill("leaving-notes", "What this bot learned.", "Body of the procedure.")).await;
    assert!(saved.get("isError").is_none(), "{saved}");
    assert!(data.join("skills").join("leaving-notes").is_dir());
    st.active_turns.lock().await.remove(sid);
    st.db.update_session_state(sid, "idle").unwrap();
    dispatch(&st, "agent.purge", json!({"agentId":writer,"confirm":true})).await.unwrap();
    assert!(st.db.skills_get("leaving-notes").is_err(), "the row went with the bot");
    assert!(!data.join("skills").join("leaving-notes").exists(), "and so did its folder");
    let _ = std::fs::remove_dir_all(&data);
}
