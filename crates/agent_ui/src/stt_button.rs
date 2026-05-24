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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SttState {
    Idle,
    Listening,
    Processing,
}

pub struct SttButton {
    pub state: SttState,
    stt_service: Option<Arc<Mutex<SttService>>>,
    _task: Option<Task<()>>,
}

impl SttButton {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self {
            state: SttState::Idle,
            stt_service: None,
            _task: None,
        }
    }

    pub fn toggle_listening(&mut self, cx: &mut Context<Self>) {
        if self.state == SttState::Listening {
            self.stop_listening(cx);
        } else if self.state == SttState::Idle {
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
            self.state = SttState::Listening;
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
                        this.update(cx, |this: &mut SttButton, cx| {
                            cx.emit(SttEvent::Error(format!("Download failed: {}", e)));
                            this.state = SttState::Idle;
                            cx.notify();
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
                        this.update(cx, |this: &mut SttButton, cx| {
                            cx.emit(SttEvent::Error(e.to_string()));
                            this.state = SttState::Idle;
                            cx.notify();
                        }).ok()
                    });
                }
            }));
        }
    }

    fn stop_listening(&mut self, cx: &mut Context<Self>) {
        self.state = SttState::Processing;
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
                            this.state = SttState::Idle;
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
        let (icon, status_text) = match self.state {
            SttState::Idle => (IconName::MicMute, None),
            SttState::Listening => (IconName::Mic, Some("Listening...")),
            SttState::Processing => (IconName::Mic, Some("Processing...")),
        };

        let tooltip = match self.state {
            SttState::Idle => "Start Listening (STT)",
            SttState::Listening => "Stop Listening",
            SttState::Processing => "Transcribing...",
        };

        div()
            .flex()
            .flex_col()
            .items_center()
            .child(
                IconButton::new("stt-button", icon)
                    .on_click(cx.listener(|this, _, _window, cx| this.toggle_listening(cx)))
                    .tooltip(Tooltip::text(tooltip)),
            )
            .when_some(status_text, |el, text| {
                el.child(
                    div()
                        .text_xs()
                        .text_color(gpui::rgba(0xffa500ff)) // amber color matching /nir theme
                        .child(text)
                )
            })
    }
}
