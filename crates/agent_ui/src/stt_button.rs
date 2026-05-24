use futures::lock::Mutex;
use gpui::{
    Context, EventEmitter, IntoElement, Render, Task, Window, div, prelude::*,
};
use prompt_stt::SttService;
use std::sync::Arc;
use ui::{ButtonCommon, Clickable, IconButton, IconName, Tooltip};

pub enum SttEvent {
    Transcription(String),
    Error(String),
}

pub struct SttButton {
    is_listening: bool,
    stt_service: Option<Arc<Mutex<SttService>>>,
    _task: Option<Task<()>>,
}

impl SttButton {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self {
            is_listening: false,
            stt_service: None,
            _task: None,
        }
    }

    pub fn toggle_listening(&mut self, cx: &mut Context<Self>) {
        if self.is_listening {
            self.stop_listening(cx);
        } else {
            self.start_listening(cx);
        }
    }

    fn start_listening(&mut self, cx: &mut Context<Self>) {
        if self.stt_service.is_none() {
            match SttService::new() {
                Ok(service) => {
                    self.stt_service = Some(Arc::new(Mutex::new(service)));
                }
                Err(e) => {
                    log::error!("Failed to initialize STT service: {}", e);
                    cx.emit(SttEvent::Error(format!("Failed to init STT: {}", e)));
                    return;
                }
            }
        }

        if let Some(service_arc) = self.stt_service.clone() {
            self.is_listening = true;
            cx.notify();

            self._task = Some(cx.spawn(async move |this, cx| {
                let needs_download = {
                    let service = service_arc.lock().await;
                    !service.is_ready()
                };

                if needs_download {
                    log::info!("First run - Downloading STT models (39MB)... Please wait.");
                }

                let mut service = service_arc.lock().await;
                
                // Trigger download if needed
                if let Err(e) = service.download_models_if_needed().await {
                    log::error!("STT download failed: {}", e);
                    let _ = gpui::AsyncApp::update(cx, |cx| {
                        this.update(cx, |_: &mut SttButton, cx| {
                            cx.emit(SttEvent::Error(format!("Download failed: {}", e)))
                        }).ok()
                    });
                    return;
                }

                if needs_download {
                    log::info!("STT Download complete! Listening...");
                }

                if let Err(e) = service.start_listening() {
                    log::error!("STT start listening failed: {}", e);
                    let _ = gpui::AsyncApp::update(cx, |cx| {
                        this.update(cx, |_: &mut SttButton, cx| {
                            cx.emit(SttEvent::Error(e.to_string()))
                        }).ok()
                    });
                }
            }));
        }
    }

    fn stop_listening(&mut self, cx: &mut Context<Self>) {
        self.is_listening = false;
        cx.notify();
        if let Some(service_arc) = self.stt_service.clone() {
            self._task = Some(cx.spawn(async move |this, cx| {
                    let service_arc_clone = service_arc.clone();
                    let result = cx.background_executor().spawn(async move {
                        let mut service = service_arc_clone.lock().await;
                        service.stop_listening()
                    }).await;

                    let _ = gpui::AsyncApp::update(cx, |cx| {
                        this.update(cx, |this: &mut SttButton, cx| {
                            match result {
                                Ok(text) => cx.emit(SttEvent::Transcription(text)),
                                Err(e) => cx.emit(SttEvent::Error(e.to_string())),
                            }
                            this.is_listening = false;
                            cx.notify();
                        })
                        .ok()
                    });
            }));
        }
    }
}

impl EventEmitter<SttEvent> for SttButton {}

impl Render for SttButton {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let icon = if self.is_listening {
            IconName::Mic // Assuming a Mic icon exists
        } else {
            IconName::MicMute
        };

        let tooltip = if self.is_listening {
            "Stop Listening"
        } else {
            "Start Listening (STT)"
        };

        div().child(
            IconButton::new("stt-button", icon)
                .on_click(cx.listener(|this, _, _window, cx| this.toggle_listening(cx)))
                .tooltip(Tooltip::text(tooltip)),
        )
    }
}
