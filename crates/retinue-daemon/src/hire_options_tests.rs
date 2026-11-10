// Hire options (9 November 2026): the name, photo, personality and skills a
// preset employee is hired with, hiring without a project, and the personality
// on an employee. Included into the daemon's test module like `phase9_tests.rs`,
// so it shares `state`, `dispatch`, `json!` and `Uuid`.

/// A real data folder for the test, so installed library skills land in it and
/// not in the system temp folder.
fn hire_data_dir(st: &AppState) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("retinue-hire-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    *st.team.data_dir.lock().unwrap() = Some(dir.clone());
    dir
}

fn skills_on(st: &AppState, agent_id: &str) -> Vec<String> {
    skills::skills_for_agent(st, Some(agent_id)).unwrap().iter().map(|row| row["name"].as_str().unwrap().to_owned()).collect()
}

#[tokio::test]
async fn hiring_with_options_sets_name_photo_personality_and_skills() {
    let (st, _events) = state();
    let dir = hire_data_dir(&st);
    let hired = dispatch(&st, "project.hire", json!({
        "presetId": "ada", "name": "  Adaline  ", "photo": "photo-02",
        "personality": {"id": "custom", "prompt": "  Speak like a ship's radio operator.  "},
        "skills": ["tdd", "code-review"], "thinking": "high",
    })).await.unwrap();
    assert_eq!(hired["created"], true);
    assert!(hired["member"].is_null(), "no project, no membership");
    let agent = st.db.agent_get(hired["agentId"].as_str().unwrap()).unwrap();
    assert_eq!(agent["name"], "Adaline");
    assert_eq!(agent["thinking"], "high", "the hire dialog's effort is kept");
    assert_eq!(agent["look"]["photo"], "photo-02");
    assert_eq!(agent["look"]["icon"], "char-ada-lovelace");
    assert_eq!(agent["personality"], json!({"id": "custom", "prompt": "Speak like a ship's radio operator."}));
    assert_eq!(hired["agent"]["personality"], agent["personality"]);
    let instructions = team::runtime_instructions(&st, &agent, &json!({"kind":"main"}), std::path::Path::new("."));
    assert!(instructions.contains("## How you communicate\nSpeak like a ship's radio operator."), "{instructions}");
    assert!(instructions.trim_end_matches("</retinue_team>").trim_end().ends_with("Speak like a ship's radio operator."));
    let mut on = skills_on(&st, agent["id"].as_str().unwrap());
    on.sort();
    assert_eq!(on, ["code-review", "tdd"], "only the chosen skills are on for this employee");
    assert!(dir.join("skills").join("tdd").join("SKILL.md").is_file(), "the files were placed in the pool");
    assert_eq!(st.db.skills_get("tdd").unwrap()["source"], "folder");
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn hiring_defaults_come_from_the_preset_and_skills_stay_with_their_employee() {
    let (st, _events) = state();
    let dir = hire_data_dir(&st);
    let existing = st.db.agent_create(&json!({"name":"Existing","runtimeId":"rt-test"})).unwrap().0["id"].as_str().unwrap().to_owned();
    let ada = dispatch(&st, "project.hire", json!({"presetId":"ada"})).await.unwrap()["agentId"].as_str().unwrap().to_owned();
    let preset = presets::find("ada").unwrap();
    assert_eq!(st.db.agent_get(&ada).unwrap()["personality"]["id"], preset.personality.as_str());
    assert_eq!(st.db.agent_get(&ada).unwrap()["personality"]["prompt"], presets::personality(&preset.personality).unwrap().prompt.as_str());
    let mut wanted = preset.skills.default.clone();
    wanted.sort();
    let mut on = skills_on(&st, &ada);
    on.sort();
    assert_eq!(on, wanted, "the preset's default skills are on");
    assert!(skills_on(&st, &existing).is_empty(), "a library skill is off for the bots that were already here");
    // A second employee choosing one skill gets only that one, and Ada keeps hers.
    let wolfie = dispatch(&st, "project.hire", json!({"presetId":"wolfie","skills":["tdd"]})).await.unwrap()["agentId"].as_str().unwrap().to_owned();
    assert_eq!(skills_on(&st, &wolfie), ["tdd"]);
    let mut again = skills_on(&st, &ada);
    again.sort();
    assert_eq!(again, wanted);
    // No skills at all, and no personality.
    let none = dispatch(&st, "project.hire", json!({"presetId":"zip","skills":[],"personality":null})).await.unwrap()["agentId"].as_str().unwrap().to_owned();
    assert!(skills_on(&st, &none).is_empty());
    assert!(st.db.agent_get(&none).unwrap()["personality"].is_null());
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn a_worn_photo_is_refused_and_a_taken_preset_photo_is_dropped() {
    let (st, _events) = state();
    dispatch(&st, "project.hire", json!({"presetId":"ada"})).await.unwrap();
    let refused = dispatch(&st, "project.hire", json!({"presetId":"wolfie","photo":"photo-17"})).await.unwrap_err();
    assert_eq!(refused.1.code, "conflict");
    assert!(st.db.agent_with_preset("wolfie").unwrap().is_none(), "nothing was made");
    // Wolfie's own photo is worn by someone else: the hire goes ahead without one.
    st.db.agent_create(&json!({"name":"Wearer","runtimeId":"rt-test","look":{"shape":"circle","photo":presets::find("wolfie").unwrap().photo}})).unwrap();
    let hired = dispatch(&st, "project.hire", json!({"presetId":"wolfie"})).await.unwrap();
    let agent = st.db.agent_get(hired["agentId"].as_str().unwrap()).unwrap();
    assert!(agent["look"].get("photo").is_none(), "{}", agent["look"]);
    assert_eq!(agent["look"]["icon"], "char-wolfgang-amadeus-mozart");
    // An empty photo means none, and a bad one is refused.
    let blank = dispatch(&st, "project.hire", json!({"presetId":"wu","photo":""})).await.unwrap();
    assert!(st.db.agent_get(blank["agentId"].as_str().unwrap()).unwrap()["look"].get("photo").is_none());
    assert_eq!(dispatch(&st, "project.hire", json!({"presetId":"nova","photo":"https://example.com/a.png"})).await.unwrap_err().1.code, "invalid_argument");
}

#[tokio::test]
async fn hire_options_are_validated_before_anything_is_made() {
    let (st, _events) = state();
    let dir = hire_data_dir(&st);
    for (body, what) in [
        (json!({"presetId":"ada","name":""}), "empty name"),
        (json!({"presetId":"ada","name":"x".repeat(41)}), "long name"),
        (json!({"presetId":"ada","name":5}), "name type"),
        (json!({"presetId":"ada","personality":{"id":"pirate"}}), "unknown personality"),
        (json!({"presetId":"ada","personality":{"id":"custom"}}), "custom without a prompt"),
        (json!({"presetId":"ada","personality":{"id":"custom","prompt":"x".repeat(2001)}}), "long prompt"),
        (json!({"presetId":"ada","personality":"warm"}), "personality type"),
        (json!({"presetId":"ada","skills":["no-such-skill"]}), "unknown skill"),
        (json!({"presetId":"ada","skills":"tdd"}), "skills type"),
    ] {
        let error = dispatch(&st, "project.hire", body).await.unwrap_err();
        assert_eq!(error.1.code, "invalid_argument", "{what}");
    }
    let too_many = skill_library::list().iter().take(16).map(|skill| skill["id"].clone()).collect::<Vec<_>>();
    assert_eq!(too_many.len(), 16);
    assert_eq!(dispatch(&st, "project.hire", json!({"presetId":"ada","skills":too_many})).await.unwrap_err().1.code, "invalid_argument");
    assert!(st.db.agent_list(false, None).unwrap().is_empty(), "no refusal left an employee behind");
    // A shipped personality takes its own prompt when only the id is given.
    let warm = dispatch(&st, "project.hire", json!({"presetId":"ada","personality":{"id":"warm"},"skills":[]})).await.unwrap();
    assert_eq!(warm["agent"]["personality"]["prompt"], presets::personality("warm").unwrap().prompt.as_str());
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn a_taken_chosen_name_gets_a_number_too() {
    let (st, _events) = state();
    st.db.agent_create(&json!({"name":"Boss","runtimeId":"rt-test"})).unwrap();
    let hired = dispatch(&st, "project.hire", json!({"presetId":"ada","name":"boss"})).await.unwrap();
    assert_eq!(st.db.agent_get(hired["agentId"].as_str().unwrap()).unwrap()["name"], "boss 2");
}

#[tokio::test]
async fn hiring_into_a_project_still_adds_the_membership_and_the_project_must_exist() {
    let (st, _events) = state();
    let project = st.db.project_create("Hire", Some(".")).unwrap().0["id"].as_str().unwrap().to_owned();
    let hired = dispatch(&st, "project.hire", json!({"projectId":project,"presetId":"ada","name":"Adaline"})).await.unwrap();
    assert!(hired["member"].is_object());
    assert_eq!(st.db.project_members(&project).unwrap().len(), 1);
    assert_eq!(dispatch(&st, "project.hire", json!({"projectId":"missing","presetId":"wolfie"})).await.unwrap_err().1.code, "not_found");
    assert_eq!(dispatch(&st, "project.hire", json!({"projectId":7,"presetId":"wolfie"})).await.unwrap_err().1.code, "invalid_argument");
    // A reused employee ignores the options and reports that it was not created.
    let again = dispatch(&st, "project.hire", json!({"presetId":"ada","name":"Other"})).await.unwrap();
    assert_eq!(again["created"], false);
    assert_eq!(again["agent"]["name"], "Adaline");
}

#[tokio::test]
async fn personality_is_set_changed_cleared_validated_and_travels_in_a_backup() {
    let (st, _events) = state();
    let created = dispatch(&st, "agent.create", json!({"name":"Pat","runtimeId":"rt-test","personality":{"id":"blunt"}})).await.unwrap()["agent"].clone();
    assert_eq!(created["personality"]["id"], "blunt");
    assert_eq!(created["personality"]["prompt"], presets::personality("blunt").unwrap().prompt.as_str());
    let id = created["id"].as_str().unwrap().to_owned();
    let updated = dispatch(&st, "agent.update", json!({"agentId":id,"personality":{"id":"custom","prompt":"Be brief."}})).await.unwrap()["agent"].clone();
    assert_eq!(updated["personality"], json!({"id":"custom","prompt":"Be brief."}));
    assert_eq!(st.db.agent_get(&id).unwrap()["personality"], updated["personality"]);
    // An update that does not mention it keeps it.
    dispatch(&st, "agent.update", json!({"agentId":id,"description":"Still Pat."})).await.unwrap();
    assert_eq!(st.db.agent_get(&id).unwrap()["personality"]["prompt"], "Be brief.");
    for bad in [json!({"id":"nope"}), json!({"id":"custom"}), json!({"id":"custom","prompt":"x".repeat(2001)}), json!("blunt"), json!({"id":"warm","extra":1})] {
        assert_eq!(dispatch(&st, "agent.update", json!({"agentId":id,"personality":bad})).await.unwrap_err().1.code, "invalid_argument");
    }
    assert_eq!(st.db.agent_get(&id).unwrap()["personality"]["prompt"], "Be brief.", "a refused update changed nothing");
    // A backup carries it.
    let exported = dispatch(&st, "backup.export", json!({"include":{"memory":false,"skills":false,"automations":false,"settings":false}})).await.unwrap();
    let document = exported["document"].clone();
    assert_eq!(document["contents"]["bots"][0]["personality"], json!({"id":"custom","prompt":"Be brief."}));
    let (target, _) = state();
    let preview = preview_of(&target, document).await;
    import_of(&target, &preview, json!({})).await.unwrap();
    let restored = target.db.agent_list(false, None).unwrap().into_iter().find(|agent| agent["name"] == "Pat").unwrap();
    assert_eq!(restored["personality"], json!({"id":"custom","prompt":"Be brief."}), "a restored bot keeps its personality");
    let cleared = dispatch(&st, "agent.update", json!({"agentId":id,"personality":null})).await.unwrap()["agent"].clone();
    assert!(cleared["personality"].is_null());
    let bare = team::runtime_instructions(&st, &st.db.agent_get(&id).unwrap(), &json!({"kind":"main"}), std::path::Path::new("."));
    assert!(!bare.contains("How you communicate"), "no personality, no section");
}

#[tokio::test]
async fn preset_list_carries_personalities_skills_and_the_library() {
    let (st, _events) = state();
    let listed = dispatch(&st, "preset.list", json!({})).await.unwrap();
    assert_eq!(listed["personalities"].as_array().unwrap().len(), 10);
    let library = listed["skillLibrary"].as_array().unwrap();
    assert!(library.len() >= 20);
    for skill in library { for key in ["id", "name", "description", "licence", "author"] { assert!(skill[key].is_string(), "{skill}"); } }
    let ada = listed["presets"].as_array().unwrap().iter().find(|preset| preset["id"] == "ada").unwrap();
    assert_eq!(ada["personality"], "steady");
    let default = ada["skills"]["default"].as_array().unwrap();
    assert!(!default.is_empty() && default.len() <= 10);
    for card in default.iter().chain(ada["skills"]["optional"].as_array().unwrap()) {
        assert!(card["id"].is_string() && card["name"].is_string() && card["description"].is_string(), "{card}");
        assert!(library.iter().any(|skill| skill["id"] == card["id"]), "{card} is in the library");
    }
}
