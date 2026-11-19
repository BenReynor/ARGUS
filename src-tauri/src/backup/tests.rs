use super::*;

async fn database() -> SqlitePool {
    let p = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db::run_migrations(&p).await.unwrap();
    p
}
async fn populated() -> SqlitePool {
    let p = database().await;
    let token = crypto::encrypt("fake-portable-bot-token").unwrap();
    sqlx::query("INSERT INTO bots (id,name,token_ciphertext) VALUES ('fake-bot','Fixture bot',?)")
        .bind(token)
        .execute(&p)
        .await
        .unwrap();
    let mut config = crate::state::Config::default();
    config.active_bot_id = Some("fake-bot".into());
    config.bots = db::list_bots(&p).await.unwrap();
    config.language = "es".into();
    db::set_config(&p, "config", &serde_json::to_string(&config).unwrap())
        .await
        .unwrap();
    db::set_setting(&p, "language", "es").await.unwrap();
    let settings = db::GuildSettings {
        personality_enabled: true,
        chat_personalidad: "fixture prompt".into(),
        chat_contexto: 18,
        chat_canales: vec!["fake-channel".into()],
        chat_usuarios: vec!["fake-user".into()],
        chat_ordenes: true,
        chat_ordenes_usuarios: vec!["fake-user".into()],
        chat_ordenes_confirmar: true,
        ordenes_max_acciones: 25,
        ..Default::default()
    };
    sqlx::query(
        "INSERT INTO guild_settings (bot_id,guild_id,data) VALUES ('fake-bot','fake-guild',?)",
    )
    .bind(serde_json::to_string(&settings).unwrap())
    .execute(&p)
    .await
    .unwrap();
    let mut config_json = serde_json::to_value(&config).unwrap();
    config_json["llm_chat_api_key_ciphertext"] =
        crypto::encrypt("fake-provider-key").unwrap().into();
    db::set_config(&p, "config", &config_json.to_string())
        .await
        .unwrap();
    db::set_setting(
        &p,
        "window_preferences",
        r#"{"auto_start":true,"minimize_to_tray":false}"#,
    )
    .await
    .unwrap();
    for table in ["plan_servidor", "contexto_servidor"] {
        sqlx::query(&format!("INSERT INTO {table} (bot_id,guild_id,contenido) VALUES ('fake-bot','fake-guild','fixture markdown')"))
            .execute(&p).await.unwrap();
    }
    sqlx::query("INSERT INTO conversacion_asistente (bot_id,guild_id,agente,mensajes) VALUES ('fake-bot','fake-guild','fixture-agent','[{\"role\":\"user\",\"content\":\"fixture conversation\"}]')")
        .execute(&p).await.unwrap();
    sqlx::query("INSERT INTO messages (id,guild_id,channel_id,user_id,content,role,timestamp) VALUES (1,'fake-guild','fake-channel','fake-user','fixture message','user','2026-01-01')")
        .execute(&p).await.unwrap();
    p
}

#[tokio::test]
async fn export_import_empty_database_preserves_every_table_and_secret() {
    let source = populated().await;
    let before = snapshot(&source, true).await.unwrap();
    let file = export(&source, "fixture password".into(), true)
        .await
        .unwrap();
    for secret in [
        "fake-portable-bot-token",
        "fake-provider-key",
        "fixture prompt",
        "fixture conversation",
    ] {
        assert!(!String::from_utf8_lossy(&file).contains(secret));
    }
    let target = database().await;
    let decoded = decode(file, "fixture password".into()).await.unwrap();
    restore(&target, decoded, true).await.unwrap();
    assert_eq!(before.tables, snapshot(&target, true).await.unwrap().tables);
    let stored: String = sqlx::query_scalar("SELECT token_ciphertext FROM bots")
        .fetch_one(&target)
        .await
        .unwrap();
    assert_ne!(stored, "fake-portable-bot-token");
    assert_eq!(crypto::decrypt(&stored).unwrap(), "fake-portable-bot-token");
    let found: i64 =
        sqlx::query_scalar("SELECT count(*) FROM messages_fts WHERE messages_fts MATCH 'fixture'")
            .fetch_one(&target)
            .await
            .unwrap();
    assert_eq!(found, 1);
    assert_eq!(
        db::get_setting(&target, "window_preferences")
            .await
            .unwrap(),
        db::get_setting(&source, "window_preferences")
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn wrong_password_and_corrupt_files_leave_database_untouched() {
    let pool = populated().await;
    let before = snapshot(&pool, true).await.unwrap().tables;
    let file = export(&pool, "fixture password".into(), true)
        .await
        .unwrap();
    assert!(decode(file.clone(), "incorrect password".into())
        .await
        .is_err());
    for corrupt in [vec![], b"not a backup".to_vec(), file[..30].to_vec(), {
        let mut b = file.clone();
        let end = b.len() - 5;
        b[end] ^= 1;
        b
    }] {
        assert!(decode(corrupt, "fixture password".into()).await.is_err());
    }
    assert_eq!(before, snapshot(&pool, true).await.unwrap().tables);
}

#[tokio::test]
async fn merge_preserves_local_conflicts_and_replace_clears_excluded_conversations() {
    let source = populated().await;
    let target = populated().await;
    sqlx::query("UPDATE bots SET name='Local name'")
        .execute(&target)
        .await
        .unwrap();
    restore(&target, snapshot(&source, false).await.unwrap(), false)
        .await
        .unwrap();
    assert_eq!(db::list_bots(&target).await.unwrap()[0].name, "Local name");
    assert_eq!(
        snapshot(&target, true).await.unwrap().tables["messages"].len(),
        1
    );
    restore(&target, snapshot(&source, false).await.unwrap(), true)
        .await
        .unwrap();
    assert_eq!(db::list_bots(&target).await.unwrap()[0].name, "Fixture bot");
    assert!(snapshot(&target, true).await.unwrap().tables["messages"].is_empty());
}

#[tokio::test]
async fn invalid_schema_and_late_sql_error_roll_back_all_changes() {
    let pool = populated().await;
    let before = snapshot(&pool, true).await.unwrap().tables;
    let mut s = snapshot(&pool, true).await.unwrap();
    s.tables.get_mut("bots").unwrap()[0]["malicious_column"] = "bad".into();
    assert!(restore(&pool, s, true).await.is_err());
    let mut s = snapshot(&pool, true).await.unwrap();
    s.tables.get_mut("settings").unwrap()[0]["value"] = Value::Null;
    assert!(restore(&pool, s, true).await.is_err());
    assert_eq!(before, snapshot(&pool, true).await.unwrap().tables);
}

#[tokio::test]
async fn automatic_backups_are_daily_private_portable_and_keep_seven() {
    let pool = populated().await;
    let dir = std::env::temp_dir().join(format!("argus-backup-test-{}", uuid::Uuid::new_v4()));
    tokio::fs::create_dir_all(&dir).await.unwrap();
    // Solo las copias automáticas propias se podan.
    tokio::fs::write(dir.join("manual.argusbak"), b"manual")
        .await
        .unwrap();
    for day in 1..=9 {
        automatic(
            &pool,
            &dir,
            "fixture password".into(),
            false,
            &format!("2026-01-{day:02}"),
        )
        .await
        .unwrap();
    }
    let latest = dir.join("automatic-2026-01-09.argusbak");
    let bytes = tokio::fs::read(&latest).await.unwrap();
    automatic(&pool, &dir, "fixture password".into(), false, "2026-01-09")
        .await
        .unwrap();
    assert_eq!(bytes, tokio::fs::read(&latest).await.unwrap());
    let s = decode(bytes, "fixture password".into()).await.unwrap();
    assert!(!s.conversations);
    assert!(!s.tables.contains_key("messages"));
    let names = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect::<Vec<_>>();
    assert_eq!(names.len(), 8);
    assert!(!dir.join("automatic-2026-01-02.argusbak").exists());
    assert!(dir.join("automatic-2026-01-03.argusbak").exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&latest).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    tokio::fs::remove_dir_all(dir).await.unwrap();
}

#[tokio::test]
async fn schedule_secret_is_excluded_from_portable_backup() {
    let pool = populated().await;
    db::set_config(&pool, SCHEDULE, "private machine schedule")
        .await
        .unwrap();
    let s = snapshot(&pool, true).await.unwrap();
    assert!(s.tables["config"].iter().all(|r| r["key"] != SCHEDULE));
    restore(&pool, s, true).await.unwrap();
    assert_eq!(
        db::get_config(&pool, SCHEDULE).await.unwrap().unwrap(),
        "private machine schedule"
    );
}

#[tokio::test]
async fn json_in_user_text_is_preserved_verbatim() {
    let source = populated().await;
    let text = "{\n  \"token_ciphertext\": \"literal user text, not a credential\"\n}";
    sqlx::query("UPDATE plan_servidor SET contenido=?")
        .bind(text)
        .execute(&source)
        .await
        .unwrap();
    sqlx::query("UPDATE contexto_servidor SET contenido=?")
        .bind(text)
        .execute(&source)
        .await
        .unwrap();
    let target = database().await;
    let file = export(&source, "fixture password".into(), true)
        .await
        .unwrap();
    restore(
        &target,
        decode(file, "fixture password".into()).await.unwrap(),
        true,
    )
    .await
    .unwrap();
    for table in ["plan_servidor", "contexto_servidor"] {
        let restored: String = sqlx::query_scalar(&format!("SELECT contenido FROM {table}"))
            .fetch_one(&target)
            .await
            .unwrap();
        assert_eq!(restored, text);
    }
}
