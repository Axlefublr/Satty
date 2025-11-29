use anyhow::{Context, Result};
use gdk_pixbuf::{gio, Pixbuf};
use gio::prelude::*;
use relm4::ComponentSender;

use crate::ipc::{IpcMessage, IpcResponse, IpcServer};
use crate::{App, AppInput};

pub struct DaemonServer {
    server: IpcServer,
}

impl DaemonServer {
    pub async fn new() -> Result<Self> {
        let server = IpcServer::new();
        eprintln!("Daemon server initialized");
        Ok(Self { server })
    }

    pub async fn run(self, sender: ComponentSender<App>) -> Result<()> {
        let connection = gio::bus_get_future(gio::BusType::Session)
            .await
            .context("Failed to connect to session bus")?;

        let request_result = connection
            .call_future(
                Some("org.freedesktop.DBus"),
                "/org/freedesktop/DBus",
                "org.freedesktop.DBus",
                "RequestName",
                Some(&(
                    "com.gabm.satty",
                    1u32 | 4u32,
                ).to_variant()),
                Some(&glib::VariantTy::new("(u)").unwrap()),
                gio::DBusCallFlags::NONE,
                -1,
            )
            .await
            .context("Failed to request DBus name")?;

        let (name_result,): (u32,) = request_result.get().context("Failed to parse name request result")?;

        match name_result {
            1 => eprintln!("Daemon listening on DBus: com.gabm.satty"),
            2 => return Err(anyhow::anyhow!("Daemon already running on DBus")),
            3 => return Err(anyhow::anyhow!("Name already exists on DBus")),
            4 => eprintln!("Daemon became primary owner on DBus"),
            _ => return Err(anyhow::anyhow!("Unknown name request result: {}", name_result)),
        }

        let sender_clone = sender.clone();
        self.server
            .register_object(&connection, move |message| {
                let sender = sender_clone.clone();
                match &message {
                    IpcMessage::Ping => {
                        IpcResponse::Pong
                    }
                    IpcMessage::Shutdown => {
                        glib::spawn_future_local(async move {
                            std::process::exit(0);
                        });
                        IpcResponse::Ok
                    }
                    _ => {
                        glib::spawn_future_local(glib::clone!(
                            #[strong]
                            sender,
                            async move {
                                if let Err(e) = Self::handle_message(message, &sender).await {
                                    eprintln!("Error handling message: {}", e);
                                }
                            }
                        ));
                        IpcResponse::Ok
                    }
                }
            })
            .context("Failed to register DBus object")?;
        Ok(())
    }

    async fn handle_message(message: IpcMessage, sender: &ComponentSender<App>) -> Result<()> {
        match message {
            IpcMessage::LoadImage { filename } => {
                let sender = sender.clone();
                glib::spawn_future_local(async move {
                    match Self::load_pixbuf_from_file(&filename) {
                        Ok(pixbuf) => {
                            sender.input(AppInput::LoadNewImage(pixbuf));
                            sender.input(AppInput::ShowWindow);

                        }
                        Err(e) => {
                            eprintln!("Failed to load image: {}", e);
                            sender.input(AppInput::HideWindow);
                        }
                    }
                });
            }
            _ => {}
        }
        Ok(())
    }

    fn load_pixbuf_from_file(filename: &str) -> Result<Pixbuf> {
        Pixbuf::from_file(filename)
            .context(format!("Failed to load image from file: {}", filename))
    }
}
