use super::*;

#[tokio::test]
async fn loop_slash_command_with_args_submits_task_text_and_arms_loop() {
    let (mut chat, mut rx, mut op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.thread_id = Some(ThreadId::new());

    chat.bottom_pane.set_composer_text(
        "/loop 15m --timeout 2h finish the migration".to_string(),
        Vec::new(),
        Vec::new(),
    );
    chat.handle_key_event(KeyEvent::from(KeyCode::Enter));

    let items = match next_submit_op(&mut op_rx) {
        Op::UserTurn { items, .. } => items,
        other => panic!("expected Op::UserTurn, got {other:?}"),
    };
    let UserInput::Text { text, .. } = &items[0] else {
        panic!("expected UserInput::Text, got {:?}", items[0]);
    };
    assert!(text.starts_with("finish the migration\n\nThis task is running in `/loop` mode."));
    assert!(text.contains("every 15m"));
    assert!(text.contains("times out after 2h"));

    let active_loop = chat.active_loop.as_ref().expect("loop should be armed");
    assert_eq!(active_loop.user_message.text, "finish the migration");

    let rendered = drain_insert_history(&mut rx)
        .iter()
        .map(|lines| lines_to_single_string(lines))
        .collect::<String>();
    assert!(rendered.contains("Loop armed"));
    assert!(rendered.contains("finish the migration"));
}

#[tokio::test]
async fn active_loop_retries_when_due_and_warns_on_timeout() {
    let (mut chat, mut rx, mut op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.thread_id = Some(ThreadId::new());

    chat.bottom_pane.set_composer_text(
        "/loop 15m --timeout 2h finish the migration".to_string(),
        Vec::new(),
        Vec::new(),
    );
    chat.handle_key_event(KeyEvent::from(KeyCode::Enter));
    let _ = next_submit_op(&mut op_rx);
    let _ = drain_insert_history(&mut rx);

    chat.bottom_pane.set_task_running(false);
    let active_loop = chat.active_loop.as_mut().expect("loop should be armed");
    active_loop.next_retry_at = Instant::now();
    chat.pre_draw_tick();

    let items = match next_submit_op(&mut op_rx) {
        Op::UserTurn { items, .. } => items,
        other => panic!("expected Op::UserTurn, got {other:?}"),
    };
    let UserInput::Text { text, .. } = &items[0] else {
        panic!("expected UserInput::Text, got {:?}", items[0]);
    };
    assert!(text.starts_with("finish the migration\n\nThis task is running in `/loop` mode."));

    let active_loop = chat
        .active_loop
        .as_mut()
        .expect("loop should still be armed");
    active_loop.deadline = Instant::now();
    active_loop.next_retry_at = Instant::now();
    chat.pre_draw_tick();

    assert!(chat.active_loop.is_none());
    assert_no_submit_op(&mut op_rx);
    let history = drain_insert_history(&mut rx);
    let rendered = history
        .iter()
        .map(|lines| lines_to_single_string(lines))
        .collect::<String>();
    assert!(rendered.contains("Loop timed out after 2h"));
}
