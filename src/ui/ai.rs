use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{
    Align, Box as GtkBox, Button, Entry, Frame, Image, Orientation, ProgressBar,
    ScrolledWindow, TextView, WrapMode,
};
use libadwaita as adw;
use adw::prelude::*;
use adw::{ActionRow, Clamp, ComboRow, ExpanderRow, PasswordEntryRow, PreferencesGroup, Toast, ToastOverlay};
use std::rc::Rc;
use std::sync::mpsc;
use crate::ai::{
    AIClient, ChatMessage, LocalAgentBridge, LocalServerDetector,
    SafetyGuard, AIOptimizerEngine,
};
use crate::backend::host_bridge::HostBridge;
use crate::backend::settings::AppSettings;

pub struct AIPage;

impl AIPage {
    pub fn build(toast_overlay: Option<ToastOverlay>) -> ScrolledWindow {
        let scroller = ScrolledWindow::builder()
            .vexpand(true)
            .hexpand(true)
            .kinetic_scrolling(true)
            .build();

        let root_box = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(20)
            .margin_top(24)
            .margin_bottom(24)
            .margin_start(24)
            .margin_end(24)
            .build();

        let toast_rc = Rc::new(toast_overlay);

        // ---------------------------------------------------------
        // 1. One-Click AI Optimization Triage
        // ---------------------------------------------------------
        let optimizer_group = PreferencesGroup::builder()
            .title("AI System Optimizer")
            .description("Real-time GNU/Linux tuning and automated diagnostic fixes via local agents or model APIs")
            .build();

        let triage_action_row = ActionRow::builder()
            .title("System Health &amp; Performance Triage")
            .subtitle("Analyze kernel schedulers, memory pressure, storage caches, and systemd units")
            .build();
        triage_action_row.add_prefix(&Image::from_icon_name("starred-symbolic"));

        let run_triage_btn = Button::builder()
            .label("Run AI Optimization Triage")
            .css_classes(vec!["suggested-action"])
            .valign(Align::Center)
            .build();
        triage_action_row.add_suffix(&run_triage_btn);
        optimizer_group.add(&triage_action_row);

        let triage_progress = ProgressBar::builder()
            .visible(false)
            .pulse_step(0.1)
            .margin_start(12)
            .margin_end(12)
            .build();
        optimizer_group.add(&ActionRow::builder().child(&triage_progress).build());

        let recs_box = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(10)
            .margin_top(4)
            .margin_bottom(8)
            .build();
        optimizer_group.add(&ActionRow::builder().child(&recs_box).build());

        root_box.append(&optimizer_group);

        // ---------------------------------------------------------
        // 2. Local Agents & Local Runners Detection
        // ---------------------------------------------------------
        let agents_group = PreferencesGroup::builder()
            .title("Local Coding Agents &amp; Model Servers")
            .description("Automatically bridges to local sessions (Antigravity, OpenClaude, Pi-Agent, Codex) or local runners (LM Studio, Ollama, llama.cpp)")
            .build();

        let agents_expander = ExpanderRow::builder()
            .title("Detected Local Agents &amp; Local Runners")
            .subtitle("Click to inspect active local sessions and runners")
            .build();

        let scan_btn = Button::builder()
            .label("Scan Local Agents")
            .css_classes(vec!["pill"])
            .valign(Align::Center)
            .build();
        agents_expander.add_suffix(&scan_btn);

        let detected_box = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(6)
            .margin_top(6)
            .margin_bottom(6)
            .build();
        agents_expander.add_row(&ActionRow::builder().child(&detected_box).build());
        agents_group.add(&agents_expander);
        root_box.append(&agents_group);

        // ---------------------------------------------------------
        // 3. Provider & Model Configuration
        // ---------------------------------------------------------
        let provider_group = PreferencesGroup::builder()
            .title("AI Provider &amp; Model Settings")
            .description("Configure active LLM source (OpenAI, Claude, Gemini, Cursor, LM Studio, Ollama, or Local Agent Session)")
            .build();

        let provider_model = gtk4::StringList::new(&[
            "Google DeepMind Antigravity (Local Agent Session)",
            "OpenClaude (Local Agent Session)",
            "Pi-Agent (Local Agent Session)",
            "Codex (Local Agent Session)",
            "Cursor (IDE / Background Agent)",
            "LM Studio (Local Offline — localhost:1234)",
            "Ollama (Local Offline — localhost:11434)",
            "llama.cpp Server (Local Offline — localhost:8080)",
            "OpenAI API (Cloud)",
            "Anthropic Claude API (Cloud)",
            "Google Gemini API (Cloud)",
            "Custom OpenAI-Compatible API",
        ]);

        let saved_cfg = AppSettings::get();

        let initial_provider_idx = match saved_cfg.ai_provider.as_str() {
            "antigravity" => 0,
            "openclaude" => 1,
            "pi-agent" => 2,
            "codex" => 3,
            "cursor" => 4,
            "lmstudio" => 5,
            "ollama" => 6,
            "llamacpp" => 7,
            "openai" => 8,
            "claude" => 9,
            "gemini" => 10,
            _ => 8,
        };

        let provider_row = ComboRow::builder()
            .title("Active Provider / Source")
            .subtitle("Select inference backend or local agent session")
            .model(&provider_model)
            .selected(initial_provider_idx)
            .build();
        provider_group.add(&provider_row);

        let url_row = ActionRow::builder()
            .title("Endpoint URL / Agent ID")
            .subtitle("API Base URL or Local Agent Protocol")
            .build();
        let url_entry = Entry::builder()
            .text(&saved_cfg.ai_endpoint)
            .hexpand(true)
            .valign(Align::Center)
            .build();
        url_row.add_suffix(&url_entry);
        provider_group.add(&url_row);

        let key_row = PasswordEntryRow::builder()
            .title("API Key (Optional for local agents / local runners)")
            .text(&saved_cfg.ai_api_key)
            .build();
        provider_group.add(&key_row);

        let model_row = ActionRow::builder()
            .title("Model Identifier")
            .subtitle("Model identifier passed in API requests (e.g. gpt-4o, claude-3-7-sonnet, nemotron)")
            .build();
        let model_entry = Entry::builder()
            .text(&saved_cfg.ai_model)
            .hexpand(true)
            .valign(Align::Center)
            .build();
        model_row.add_suffix(&model_entry);
        provider_group.add(&model_row);

        let test_row = ActionRow::builder()
            .title("Connection Diagnostic")
            .subtitle("Verify connectivity with selected provider or local session")
            .build();
        let test_btn = Button::builder()
            .label("Test Connection")
            .valign(Align::Center)
            .build();
        test_row.add_suffix(&test_btn);
        provider_group.add(&test_row);

        root_box.append(&provider_group);

        // ---------------------------------------------------------
        // 4. Interactive Linux Assistant
        // ---------------------------------------------------------
        let task_group = PreferencesGroup::builder()
            .title("Interactive Linux Assistant")
            .description("Ask system optimization, kernel tuning, or administration questions")
            .build();

        let prompt_row = ActionRow::builder()
            .title("Optimization or Command Prompt")
            .build();

        let prompt_entry = Entry::builder()
            .placeholder_text("e.g. 'Optimize BORE scheduler' or 'Find files over 100MB'")
            .hexpand(true)
            .valign(Align::Center)
            .build();

        let ask_btn = Button::builder()
            .label("Ask AI")
            .css_classes(vec!["suggested-action"])
            .valign(Align::Center)
            .build();

        prompt_row.add_suffix(&prompt_entry);
        prompt_row.add_suffix(&ask_btn);
        task_group.add(&prompt_row);

        // Quick Optimization Chips
        let chips_box = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .spacing(8)
            .margin_top(4)
            .margin_bottom(8)
            .margin_start(12)
            .margin_end(12)
            .build();

        let chips = [
            ("Tune BORE Scheduler", "How do I optimize the CachyOS BORE scheduler for gaming and responsiveness?"),
            ("Reclaim Standby RAM", "Analyze memory pressure and show safe command to drop caches"),
            ("Triage Failed Services", "Inspect failed systemd services and show safe fix"),
            ("Find Large Files", "Find files exceeding 100MB in my home directory"),
            ("Check Listening Ports", "Display active listening network sockets and ports"),
        ];

        for (label, query) in chips {
            let chip_btn = Button::builder()
                .label(label)
                .css_classes(vec!["pill"])
                .build();

            let pe_c = prompt_entry.clone();
            let q_str = query.to_string();
            chip_btn.connect_clicked(move |_| {
                pe_c.set_text(&q_str);
            });
            chips_box.append(&chip_btn);
        }
        task_group.add(&ActionRow::builder().child(&chips_box).build());

        let response_view = TextView::builder()
            .editable(false)
            .cursor_visible(false)
            .wrap_mode(WrapMode::Word)
            .monospace(true)
            .height_request(140)
            .build();
        response_view.buffer().set_text(
            "Select an AI provider above and click 'Run AI Optimization Triage' to analyze system diagnostics.",
        );

        let frame = Frame::builder()
            .child(&response_view)
            .margin_start(12)
            .margin_end(12)
            .margin_top(8)
            .margin_bottom(8)
            .build();
        task_group.add(&ActionRow::builder().child(&frame).build());

        let proposed_row = ActionRow::builder()
            .title("Proposed Terminal Fix")
            .subtitle("No command proposed yet")
            .build();

        let exec_fix_btn = Button::builder()
            .label("Execute Fix")
            .css_classes(vec!["suggested-action"])
            .valign(Align::Center)
            .sensitive(false)
            .build();

        proposed_row.add_suffix(&exec_fix_btn);
        task_group.add(&proposed_row);

        root_box.append(&task_group);

        let clamp = Clamp::builder()
            .maximum_size(900)
            .child(&root_box)
            .build();
        scroller.set_child(Some(&clamp));

        // ---------------------------------------------------------
        // Signal Connectors & Logic
        // ---------------------------------------------------------

        // Populate Local Agents & Servers Box
        let populate_local_status = {
            let db = detected_box.clone();
            let ae = agents_expander.clone();
            move || {
                while let Some(child) = db.first_child() {
                    db.remove(&child);
                }

                let agents = LocalAgentBridge::detect_agents();
                let servers = LocalServerDetector::scan_local_servers();

                let mut active_count = 0;

                // Agents
                for agent in &agents {
                    let r = ActionRow::builder()
                        .title(&agent.name)
                        .subtitle(format!("{} | {}", if agent.installed { "Installed" } else { "Not Found" }, agent.session_info))
                        .build();

                    let icon_name = if agent.session_detected {
                        active_count += 1;
                        "emblem-ok-symbolic"
                    } else if agent.installed {
                        "emblem-default-symbolic"
                    } else {
                        "dialog-information-symbolic"
                    };
                    r.add_prefix(&Image::from_icon_name(icon_name));
                    db.append(&r);
                }

                // Servers
                for server in &servers {
                    let r = ActionRow::builder()
                        .title(&server.name)
                        .subtitle(format!(
                            "Port: {} | {} | Models: {}",
                            server.port,
                            if server.is_online { "Online" } else { "Offline" },
                            if server.detected_models.is_empty() { "None detected".to_string() } else { server.detected_models.join(", ") }
                        ))
                        .build();

                    let icon_name = if server.is_online {
                        active_count += 1;
                        "emblem-ok-symbolic"
                    } else {
                        "dialog-information-symbolic"
                    };
                    r.add_prefix(&Image::from_icon_name(icon_name));
                    db.append(&r);
                }

                ae.set_subtitle(&format!("{} active local agent(s) / runner server(s) detected", active_count));
            }
        };
        populate_local_status();

        // Scan button
        {
            let p_status = populate_local_status.clone();
            let toast_rc_scan = toast_rc.clone();
            scan_btn.connect_clicked(move |_| {
                p_status();
                if let Some(ref o) = *toast_rc_scan {
                    o.add_toast(Toast::new("Local agents & runner scan updated"));
                }
            });
        }

        // Provider sync
        {
            let ue = url_entry.clone();
            let me = model_entry.clone();
            provider_row.connect_selected_notify(move |combo| {
                let (prov_id, default_url, default_model) = match combo.selected() {
                    0 => ("antigravity", "agent://antigravity", "antigravity-local"),
                    1 => ("openclaude", "agent://openclaude", "claude-session"),
                    2 => ("pi-agent", "agent://pi-agent", "pi-session"),
                    3 => ("codex", "agent://codex", "codex-session"),
                    4 => ("cursor", "agent://cursor", "cursor-agent"),
                    5 => ("lmstudio", "http://localhost:1234/v1", "nemotron"),
                    6 => ("ollama", "http://localhost:11434/v1", "nemotron"),
                    7 => ("llamacpp", "http://localhost:8080/v1", "default"),
                    8 => ("openai", "https://api.openai.com/v1", "gpt-4o-mini"),
                    9 => ("claude", "https://api.anthropic.com/v1", "claude-3-7-sonnet-latest"),
                    10 => ("gemini", "https://generativelanguage.googleapis.com/v1beta", "gemini-2.0-flash"),
                    _ => ("custom", "http://localhost:8000/v1", "default"),
                };

                ue.set_text(default_url);
                me.set_text(default_model);

                AppSettings::update(|cfg| {
                    cfg.ai_provider = prov_id.to_string();
                    cfg.ai_endpoint = default_url.to_string();
                    cfg.ai_model = default_model.to_string();
                });
            });
        }

        // Save on entry change
        {
            url_entry.connect_changed(move |e| {
                let val = e.text().to_string();
                AppSettings::update(|cfg| cfg.ai_endpoint = val);
            });

            key_row.connect_entry_activated(move |e| {
                let val = e.text().to_string();
                AppSettings::update(|cfg| cfg.ai_api_key = val);
            });

            model_entry.connect_changed(move |e| {
                let val = e.text().to_string();
                AppSettings::update(|cfg| cfg.ai_model = val);
            });
        }

        // Test connection
        {
            let ue = url_entry.clone();
            let ke = key_row.clone();
            let me = model_entry.clone();
            let pr = provider_row.clone();
            let toast_rc_test = toast_rc.clone();
            let tb = test_btn.clone();

            test_btn.connect_clicked(move |_| {
                tb.set_sensitive(false);
                tb.set_label("Testing...");

                let prov_idx = pr.selected();
                let prov_name = match prov_idx {
                    0 => "antigravity",
                    1 => "openclaude",
                    2 => "pi-agent",
                    3 => "codex",
                    4 => "cursor",
                    5 => "lmstudio",
                    6 => "ollama",
                    7 => "llamacpp",
                    8 => "openai",
                    9 => "claude",
                    10 => "gemini",
                    _ => "custom",
                };

                let url = ue.text().to_string();
                let key = ke.text().to_string();
                let model = me.text().to_string();

                let (tx, rx) = mpsc::channel();
                std::thread::spawn(move || {
                    let client = AIClient::new(prov_name, &url, &key, &model);
                    let messages = vec![ChatMessage {
                        role: "user".to_string(),
                        content: "Ping: respond with 'OK'".to_string(),
                    }];
                    let res = client.chat(&messages);
                    let _ = tx.send(res);
                });

                let tb_c = tb.clone();
                let toast_c = toast_rc_test.clone();

                glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
                    if let Ok(res) = rx.try_recv() {
                        tb_c.set_sensitive(true);
                        tb_c.set_label("Test Connection");
                        match res {
                            Ok(_) => {
                                if let Some(ref o) = *toast_c {
                                    o.add_toast(Toast::new("Connection successful!"));
                                }
                            }
                            Err(e) => {
                                if let Some(ref o) = *toast_c {
                                    o.add_toast(Toast::new(&format!("Connection failed: {}", e)));
                                }
                            }
                        }
                        return glib::ControlFlow::Break;
                    }
                    glib::ControlFlow::Continue
                });
            });
        }

        // Active proposed command storage
        let active_cmd_rc: Rc<std::cell::RefCell<Option<String>>> = Rc::new(std::cell::RefCell::new(None));

        // Connect Ask AI button
        {
            let pe = prompt_entry.clone();
            let ue = url_entry.clone();
            let ke = key_row.clone();
            let me = model_entry.clone();
            let pr = provider_row.clone();
            let rv = response_view.clone();
            let p_row = proposed_row.clone();
            let eb = exec_fix_btn.clone();
            let ab = ask_btn.clone();
            let cmd_rc = active_cmd_rc.clone();

            ask_btn.connect_clicked(move |_| {
                let prompt = pe.text().to_string();
                if prompt.trim().is_empty() {
                    return;
                }

                ab.set_sensitive(false);
                ab.set_label("Thinking...");
                eb.set_sensitive(false);
                p_row.set_subtitle("Thinking...");

                let prov_idx = pr.selected();
                let prov_name = match prov_idx {
                    0 => "antigravity",
                    1 => "openclaude",
                    2 => "pi-agent",
                    3 => "codex",
                    4 => "cursor",
                    5 => "lmstudio",
                    6 => "ollama",
                    7 => "llamacpp",
                    8 => "openai",
                    9 => "claude",
                    10 => "gemini",
                    _ => "custom",
                };

                let url = ue.text().to_string();
                let key = ke.text().to_string();
                let model = me.text().to_string();

                let (tx, rx) = mpsc::channel();

                std::thread::spawn(move || {
                    let client = AIClient::new(prov_name, &url, &key, &model);
                    let messages = vec![
                        ChatMessage {
                            role: "system".to_string(),
                            content: "You are Mithrandir's AI Linux Optimizer. Propose concise, safe terminal solutions. If a command fix applies, format it as PROPOSED_COMMAND: <cmd>.".to_string(),
                        },
                        ChatMessage {
                            role: "user".to_string(),
                            content: prompt,
                        },
                    ];
                    let res = client.chat(&messages);
                    let _ = tx.send(res);
                });

                let ab_c = ab.clone();
                let rv_c = rv.clone();
                let pr_c = p_row.clone();
                let eb_c = eb.clone();
                let cmd_rc_c = cmd_rc.clone();

                glib::timeout_add_local(std::time::Duration::from_millis(150), move || {
                    if let Ok(res) = rx.try_recv() {
                        ab_c.set_sensitive(true);
                        ab_c.set_label("Ask AI");

                        match res {
                            Ok(chat_resp) => {
                                rv_c.buffer().set_text(&chat_resp.content);
                                if let Some(ref cmd) = chat_resp.proposed_command {
                                    pr_c.set_subtitle(cmd);
                                    eb_c.set_sensitive(true);
                                    *cmd_rc_c.borrow_mut() = Some(cmd.clone());
                                } else {
                                    pr_c.set_subtitle("No command proposed");
                                    eb_c.set_sensitive(false);
                                    *cmd_rc_c.borrow_mut() = None;
                                }
                            }
                            Err(e) => {
                                rv_c.buffer().set_text(&format!("Error: {}", e));
                                pr_c.set_subtitle("Query failed");
                                eb_c.set_sensitive(false);
                                *cmd_rc_c.borrow_mut() = None;
                            }
                        }
                        return glib::ControlFlow::Break;
                    }
                    glib::ControlFlow::Continue
                });
            });
        }

        // Connect Execute Fix button
        {
            let cmd_rc = active_cmd_rc.clone();
            let toast_rc_exec = toast_rc.clone();
            let eb = exec_fix_btn.clone();

            exec_fix_btn.connect_clicked(move |_| {
                if let Some(cmd) = cmd_rc.borrow().clone() {
                    let (safe, reason) = SafetyGuard::is_command_safe(&cmd);
                    if !safe {
                        if let Some(ref o) = *toast_rc_exec {
                            o.add_toast(Toast::new(&format!("Execution blocked: {}", reason)));
                        }
                        return;
                    }

                    eb.set_sensitive(false);
                    eb.set_label("Executing...");

                    let eb_clone = eb.clone();
                    let toast_clone = toast_rc_exec.clone();
                    let (tx, rx) = mpsc::channel();

                    std::thread::spawn(move || {
                        let res = HostBridge::run_command("sh", &["-c", &cmd]);
                        let _ = tx.send(res);
                    });

                    glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
                        if let Ok(res) = rx.try_recv() {
                            eb_clone.set_sensitive(true);
                            eb_clone.set_label("Execute Fix");
                            match res {
                                Ok((0, _, _)) => {
                                    if let Some(ref o) = *toast_clone {
                                        o.add_toast(Toast::new("Command executed successfully!"));
                                    }
                                }
                                Ok((code, _, err)) => {
                                    if let Some(ref o) = *toast_clone {
                                        o.add_toast(Toast::new(&format!("Command failed (code {}): {}", code, err.trim())));
                                    }
                                }
                                Err(e) => {
                                    if let Some(ref o) = *toast_clone {
                                        o.add_toast(Toast::new(&format!("Execution error: {}", e)));
                                    }
                                }
                            }
                            return glib::ControlFlow::Break;
                        }
                        glib::ControlFlow::Continue
                    });
                }
            });
        }

        // Connect Run AI Optimization Triage button
        {
            let run_btn = run_triage_btn.clone();
            let tp = triage_progress.clone();
            let rb = recs_box.clone();
            let pr = provider_row.clone();
            let ue = url_entry.clone();
            let ke = key_row.clone();
            let me = model_entry.clone();
            let toast_rc_triage = toast_rc.clone();

            run_triage_btn.connect_clicked(move |_| {
                run_btn.set_sensitive(false);
                run_btn.set_label("Analyzing System...");
                tp.set_visible(true);

                let prov_idx = pr.selected();
                let prov_name = match prov_idx {
                    0 => "antigravity",
                    1 => "openclaude",
                    2 => "pi-agent",
                    3 => "codex",
                    4 => "cursor",
                    5 => "lmstudio",
                    6 => "ollama",
                    7 => "llamacpp",
                    8 => "openai",
                    9 => "claude",
                    10 => "gemini",
                    _ => "custom",
                };

                let url = ue.text().to_string();
                let key = ke.text().to_string();
                let model = me.text().to_string();

                let (tx, rx) = mpsc::channel();

                std::thread::spawn(move || {
                    let client = AIClient::new(prov_name, &url, &key, &model);
                    let report = AIOptimizerEngine::run_ai_optimization(&client);
                    let _ = tx.send(report);
                });

                let run_btn_c = run_btn.clone();
                let tp_c = tp.clone();
                let rb_c = rb.clone();
                let toast_c = toast_rc_triage.clone();

                glib::timeout_add_local(std::time::Duration::from_millis(150), move || {
                    if let Ok(res) = rx.try_recv() {
                        run_btn_c.set_sensitive(true);
                        run_btn_c.set_label("Run AI Optimization Triage");
                        tp_c.set_visible(false);

                        while let Some(child) = rb_c.first_child() {
                            rb_c.remove(&child);
                        }

                        match res {
                            Ok(report) => {
                                if let Some(ref o) = *toast_c {
                                    o.add_toast(Toast::new("AI Optimization Triage completed!"));
                                }

                                let summary_row = ActionRow::builder()
                                    .title("System Health &amp; Telemetry Status")
                                    .subtitle(format!(
                                        "CPU: {} | RAM: {} | Kernel: {}",
                                        report.cpu_summary, report.memory_summary, report.kernel_summary
                                    ))
                                    .build();
                                summary_row.add_prefix(&Image::from_icon_name("utilities-system-monitor-symbolic"));
                                rb_c.append(&summary_row);

                                let insight_row = ActionRow::builder()
                                    .title("AI Optimization Tuning Insights")
                                    .subtitle(&report.ai_insights)
                                    .build();
                                insight_row.add_prefix(&Image::from_icon_name("starred-symbolic"));
                                rb_c.append(&insight_row);

                                for rec in report.recommendations {
                                    let r_row = ActionRow::builder()
                                        .title(&rec.title)
                                        .subtitle(format!("[{}] {}", rec.category, rec.description))
                                        .build();

                                    let icon_name = match rec.severity.as_str() {
                                        "warning" => "dialog-warning-symbolic",
                                        "recommendation" => "emblem-favorite-symbolic",
                                        _ => "dialog-information-symbolic",
                                    };
                                    r_row.add_prefix(&Image::from_icon_name(icon_name));

                                    if let Some(cmd) = rec.proposed_command {
                                        let exec_btn = Button::builder()
                                            .label("Apply Fix")
                                            .css_classes(vec!["suggested-action", "pill"])
                                            .valign(Align::Center)
                                            .build();

                                        let toast_fix = toast_c.clone();
                                        let cmd_str = cmd.clone();
                                        exec_btn.connect_clicked(move |b| {
                                            b.set_sensitive(false);
                                            let cmd_run = cmd_str.clone();
                                            let toast_run = toast_fix.clone();
                                            let b_run = b.clone();
                                            let (tx_fix, rx_fix) = mpsc::channel();

                                            std::thread::spawn(move || {
                                                let r = HostBridge::run_command("sh", &["-c", &cmd_run]);
                                                let _ = tx_fix.send(r);
                                            });

                                            glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
                                                if let Ok(r) = rx_fix.try_recv() {
                                                    b_run.set_sensitive(true);
                                                    if let Ok((code, _, err)) = r {
                                                        if code == 0 {
                                                            if let Some(ref o) = *toast_run {
                                                                o.add_toast(Toast::new("Optimization applied successfully!"));
                                                            }
                                                        } else {
                                                            if let Some(ref o) = *toast_run {
                                                                o.add_toast(Toast::new(&format!("Fix failed ({}): {}", code, err.trim())));
                                                            }
                                                        }
                                                    }
                                                    return glib::ControlFlow::Break;
                                                }
                                                glib::ControlFlow::Continue
                                            });
                                        });

                                        r_row.add_suffix(&exec_btn);
                                    }

                                    rb_c.append(&r_row);
                                }
                            }
                            Err(e) => {
                                if let Some(ref o) = *toast_c {
                                    o.add_toast(Toast::new(&format!("Optimization triage failed: {}", e)));
                                }
                            }
                        }
                        return glib::ControlFlow::Break;
                    }
                    glib::ControlFlow::Continue
                });
            });
        }

        scroller
    }
}
