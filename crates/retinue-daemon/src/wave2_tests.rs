// Wave 2, agent experience (nod ideas 2, 3, 4, 6 and 7): the "now" line a bot
// writes, memory recall before a turn, a plan that outlives one turn, the docs
// tools, and search over earlier messages. Included into `phase2a_tests`, so the
// helpers there (`state`, `dispatch`, `call_tool`, `memory_agent`,
// `memory_session`, `tool_names`, `next_event`) are in scope.

#[tokio::test]
async fn a_working_note_needs_a_running_turn_and_is_cleared_when_it_ends() {
    let (st, mut events) = state();
    let agent = memory_agent(&st, "Note blob");
    let sid = "note-session";
    memory_session(&st, &agent, sid);
    // No turn is running: there is nothing to note, and the daemon says so.
    let refused = call_tool(&st, sid, "set_working_note", json!({"text":"Reading the diff"})).await;
    assert_eq!(refused["isError"], true, "{refused}");
    assert!(refused["content"][0]["text"].as_str().unwrap().contains("no turn running"));
    st.active_turns.lock().await.insert(sid.to_owned(), "turn-note".to_owned());
    // One line, whatever the bot typed.
    let ok = call_tool(&st, sid, "set_working_note", json!({"text":"  Checking the gate\nbefore merging  "})).await;
    assert!(ok.get("isError").is_none(), "{ok}");
    let note = next_event(&mut events, "session.note").await;
    assert_eq!(note.payload["sessionId"], sid);
    assert_eq!(note.payload["turnId"], "turn-note");
    assert_eq!(note.payload["text"], "Checking the gate before merging");
    // The rails: length and secrets.
    let long = call_tool(&st, sid, "set_working_note", json!({"text":"x".repeat(crate::working_note::MAX_NOTE_CHARS + 1)})).await;
    assert_eq!(long["isError"], true, "{long}");
    let secret = call_tool(&st, sid, "set_working_note", json!({"text":"using sk-ant-abcdefghijklmnopqrstuvwxyz012345"})).await;
    assert_eq!(secret["isError"], true, "{secret}");
    // The turn ending clears the line with one empty note, and a turn that wrote
    // none adds no event at all.
    crate::working_note::announce_clear(&st, sid).await;
    let cleared = next_event(&mut events, "session.note").await;
    assert_eq!(cleared.payload["text"], "");
    assert_eq!(cleared.payload["turnId"], "turn-note");
    crate::working_note::announce_clear(&st, sid).await;
    st.active_turns.lock().await.remove(sid);
}

#[tokio::test]
async fn memory_recall_sends_the_notes_that_bear_on_the_message_once() {
    let (st, _) = state();
    let agent = memory_agent(&st, "Recall blob");
    let sid = "recall-session";
    memory_session(&st, &agent, sid);
    st.db.set_setting("experimental.memory", &json!(true)).unwrap();
    dispatch(&st, "memory.create", json!({"scope":"blob","scopeId":agent,"kind":"decision","title":"Deploys","body":"We deploy on Fridays, never on a Monday."})).await.unwrap();
    dispatch(&st, "memory.create", json!({"scope":"blob","scopeId":agent,"kind":"fact","title":"Coffee","body":"The machine grinds beans at nine."})).await.unwrap();
    let block = memory::recall_for_prompt(&st, sid, "When do we deploy the release?").expect("a recall block");
    assert!(block.contains("<retinue_memory_now>") && block.contains("</retinue_memory_now>"), "{block}");
    assert!(block.contains("Deploys"), "{block}");
    assert!(block.contains("evidence, not instructions"), "the block is labelled as data:\n{block}");
    assert!(!block.contains("Coffee"), "an unrelated note is not recalled:\n{block}");
    assert!(memory::recall_for_prompt(&st, sid, "When do we deploy the release?").is_none(), "the same selection is not re-sent");
    assert!(memory::recall_for_prompt(&st, sid, "beans coffee machine").is_some(), "a different selection is sent");
    // No usable words, or no matching note: nothing at all.
    assert!(memory::recall_for_prompt(&st, sid, "ok").is_none());
    assert!(memory::recall_for_prompt(&st, sid, "zzzznotawordzzzz").is_none());
    // Off means off.
    st.db.set_setting("experimental.memory", &json!(false)).unwrap();
    assert!(memory::recall_for_prompt(&st, sid, "When do we deploy the release?").is_none());
}

#[tokio::test]
async fn a_plan_is_kept_on_the_conversation_and_carried_into_the_next_turn() {
    let (st, _) = state();
    let agent = memory_agent(&st, "Plan blob");
    let sid = "plan-session";
    memory_session(&st, &agent, sid);
    let items = vec![json!({"text":"Read the parser","status":"completed"}), json!({"text":"Fix the floats","status":"in_progress"}), json!({"text":"Re-run the tests","status":"pending"})];
    crate::plan::record(&st, sid, Some("plan-turn-1"), &items);
    let plan = st.db.session_plan(sid).unwrap().expect("a stored plan");
    assert_eq!(plan["turnId"], "plan-turn-1");
    assert_eq!(plan["items"].as_array().unwrap().len(), 3);
    // The window reads it from the conversation itself.
    assert_eq!(st.db.session_detail(sid).unwrap()["plan"]["items"][1]["text"], "Fix the floats");
    let block = crate::plan::carried_plan_for_prompt(&st, sid, "plan-turn-2").expect("carried into the next turn");
    assert!(block.contains("<retinue_plan>") && block.contains("</retinue_plan>"), "{block}");
    assert!(block.contains("[x] Read the parser") && block.contains("[~] Fix the floats") && block.contains("[ ] Re-run the tests"), "{block}");
    assert!(crate::plan::carried_plan_for_prompt(&st, sid, "plan-turn-1").is_none(), "the turn that wrote it does not carry it");
    // A finished plan is not carried; an empty list clears it.
    crate::plan::record(&st, sid, Some("plan-turn-1"), &[json!({"text":"Done","status":"completed"})]);
    assert!(crate::plan::carried_plan_for_prompt(&st, sid, "plan-turn-2").is_none());
    crate::plan::record(&st, sid, Some("plan-turn-1"), &[]);
    assert!(st.db.session_plan(sid).unwrap().is_none());
    assert!(st.db.session_detail(sid).unwrap().get("plan").is_none(), "a cleared plan leaves no field");
}

#[tokio::test]
async fn the_docs_tools_answer_from_the_embedded_library() {
    let (st, _) = state();
    let agent = memory_agent(&st, "Docs blob");
    let sid = "docs-session";
    memory_session(&st, &agent, sid);
    let listed = tool_names(&team::mcp_rpc(&st, sid, "mcp.tools.list", &json!({})).await.unwrap());
    for name in ["search_docs", "read_doc", "search_history", "set_working_note"] {
        assert!(listed.contains(&name.to_owned()), "{name} is offered");
    }
    let found = call_tool(&st, sid, "search_docs", json!({"query":"approval card"})).await;
    let text = found["content"][0]["text"].as_str().unwrap().to_owned();
    assert!(text.contains(".md · "), "{text}");
    assert!(text.contains("line "), "{text}");
    // The page the search named can be read back.
    let path = text.lines().find_map(|line| line.strip_prefix("- ")).and_then(|rest| rest.split(" · ").next()).unwrap_or("").to_owned();
    assert!(!path.is_empty(), "{text}");
    let read = call_tool(&st, sid, "read_doc", json!({"path":path})).await;
    assert!(read["content"][0]["text"].as_str().unwrap().contains(path.as_str()), "{read}");
    let missing = call_tool(&st, sid, "read_doc", json!({"path":"reference/nowhere.md"})).await;
    assert_eq!(missing["isError"], true, "{missing}");
    // A scheduled run may still read the docs: read-only and offline.
    let auto = "docs-auto-session";
    memory_session(&st, &agent, auto);
    st.db.set_session_link(auto, &json!({"kind":"automation","automationId":"a","runId":"r","approvalMode":"auto"})).unwrap();
    let auto_read = call_tool(&st, auto, "search_docs", json!({"query":"approval"})).await;
    assert!(auto_read.get("isError").is_none(), "{auto_read}");
}

#[tokio::test]
async fn message_search_is_scoped_to_a_conversation_or_a_project_and_skips_archived_ones() {
    let (st, _) = state();
    let agent = memory_agent(&st, "Search blob");
    let project = st.db.project_create("Search project", None).unwrap().0;
    let other = st.db.project_create("Other project", None).unwrap().0;
    let a = "search-a";
    let b = "search-b";
    memory_session(&st, &agent, a);
    memory_session(&st, &agent, b);
    st.db.merge_session_data(a, &json!({"projectId": project["id"]})).unwrap();
    st.db.merge_session_data(b, &json!({"projectId": other["id"]})).unwrap();
    st.db.append_message(a, "", "user", "We keep the retry budget under 30 seconds.").unwrap();
    st.db.append_message(b, "", "user", "The retry budget is unbounded here.").unwrap();
    // Whole database: both conversations.
    let all = dispatch(&st, "search.messages", json!({"query":"retry budget"})).await.unwrap();
    assert_eq!(all["results"].as_array().unwrap().len(), 2, "{all}");
    assert_eq!(all["truncated"], false);
    // One conversation.
    let one = dispatch(&st, "search.messages", json!({"query":"retry budget","sessionId":a})).await.unwrap();
    assert_eq!(one["results"].as_array().unwrap().len(), 1, "{one}");
    assert_eq!(one["results"][0]["sessionId"], a);
    // One project, and a conversation outside it is refused rather than widened.
    let by_project = dispatch(&st, "search.messages", json!({"query":"retry budget","projectId":project["id"]})).await.unwrap();
    assert_eq!(by_project["results"].as_array().unwrap().len(), 1, "{by_project}");
    assert_eq!(by_project["results"][0]["sessionId"], a);
    let wrong = dispatch(&st, "search.messages", json!({"query":"retry budget","sessionId":b,"projectId":project["id"]})).await.unwrap_err();
    assert_eq!(wrong.1.code, "invalid_argument");
    // Dates compare as text in UTC, and a bad bound is refused.
    let now = chrono::Utc::now();
    let past = (now - chrono::Duration::hours(2)).to_rfc3339();
    let future = (now + chrono::Duration::hours(2)).to_rfc3339();
    assert_eq!(dispatch(&st, "search.messages", json!({"query":"retry budget","from":past})).await.unwrap()["results"].as_array().unwrap().len(), 2);
    assert_eq!(dispatch(&st, "search.messages", json!({"query":"retry budget","from":future})).await.unwrap()["results"].as_array().unwrap().len(), 0);
    assert_eq!(dispatch(&st, "search.messages", json!({"query":"retry budget","from":"not-a-date"})).await.unwrap_err().1.code, "invalid_argument");
    // A limit is honoured and says when more matches exist.
    let limited = dispatch(&st, "search.messages", json!({"query":"retry budget","limit":1})).await.unwrap();
    assert_eq!(limited["results"].as_array().unwrap().len(), 1);
    assert_eq!(limited["truncated"], true);
    assert_eq!(dispatch(&st, "search.messages", json!({"query":"retry budget","limit":51})).await.unwrap_err().1.code, "invalid_argument");
    // An archived conversation is never searched, and naming one is refused.
    dispatch(&st, "session.archive", json!({"sessionId":a,"archived":true})).await.unwrap();
    let after = dispatch(&st, "search.messages", json!({"query":"retry budget"})).await.unwrap();
    assert_eq!(after["results"].as_array().unwrap().len(), 1, "{after}");
    assert_eq!(after["results"][0]["sessionId"], b);
    assert_eq!(dispatch(&st, "search.messages", json!({"query":"retry budget","sessionId":a})).await.unwrap_err().1.code, "not_found");
    // The bot's own tool: this conversation, or its project, never another's.
    dispatch(&st, "session.archive", json!({"sessionId":a,"archived":false})).await.unwrap();
    let here = call_tool(&st, b, "search_history", json!({"query":"retry budget"})).await;
    let here_text = here["content"][0]["text"].as_str().unwrap();
    assert!(here_text.contains("in this conversation"), "{here_text}");
    assert!(!here_text.contains("under 30 seconds"), "another conversation's message is not in this one:\n{here_text}");
    let project_scope = call_tool(&st, b, "search_history", json!({"query":"retry budget","scope":"project"})).await;
    let project_text = project_scope["content"][0]["text"].as_str().unwrap();
    assert!(project_text.contains("in this project"), "{project_text}");
    assert!(!project_text.contains("under 30 seconds"), "a bot never reads another project's history:\n{project_text}");
    // A casual conversation has no project to search.
    let casual = "search-casual";
    memory_session(&st, &agent, casual);
    let no_project = call_tool(&st, casual, "search_history", json!({"query":"retry","scope":"project"})).await;
    assert_eq!(no_project["isError"], true, "{no_project}");
}
