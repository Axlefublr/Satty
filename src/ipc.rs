use anyhow::{Context, Result};
use gdk_pixbuf::gio;
use gio::prelude::*;
use gio::DBusConnection;
use glib::Variant;
use std::cell::RefCell;

#[derive(Debug, Clone)]
pub enum IpcMessage {
    LoadImage { filename: String },
    Shutdown,
    Ping,
}

#[derive(Debug, Clone)]
pub enum IpcResponse {
    Ok,
    Error(String),
    Pong,
}

pub const DBUS_INTERFACE_XML: &str = r#"
<node>
  <interface name='com.gabm.satty.IPC'>
    <method name='LoadImage'>
      <arg type='s' name='filename' direction='in'/>
      <arg type='s' name='response' direction='out'/>
    </method>
    <method name='Shutdown'>
      <arg type='s' name='response' direction='out'/>
    </method>
    <method name='Ping'>
      <arg type='s' name='response' direction='out'/>
    </method>
  </interface>
</node>
"#;

pub const DBUS_INTERFACE_NAME: &str = "com.gabm.satty.IPC";
pub const DBUS_OBJECT_PATH: &str = "/com/gabm/satty/IPC";

impl IpcMessage {
    pub fn from_method_call(
        method: &str,
        params: glib::Variant,
    ) -> Result<Self, glib::Error> {
        match method {
            "LoadImage" => {
                let (filename,): (String,) = params.get().ok_or_else(|| {
                    glib::Error::new(
                        gio::DBusError::InvalidArgs,
                        "Invalid filename parameter",
                    )
                })?;
                Ok(IpcMessage::LoadImage { filename })
            }
            "Shutdown" => Ok(IpcMessage::Shutdown),
            "Ping" => Ok(IpcMessage::Ping),
            _ => Err(glib::Error::new(
                gio::DBusError::UnknownMethod,
                "Unknown method",
            )),
        }
    }
}

impl IpcResponse {
    pub fn to_variant(&self) -> Variant {
        match self {
            IpcResponse::Ok => ("Ok",).to_variant(),
            IpcResponse::Error(msg) => (format!("Error: {}", msg),).to_variant(),
            IpcResponse::Pong => ("Pong",).to_variant(),
        }
    }
}

pub struct IpcServer {
    registration_id: RefCell<Option<gio::RegistrationId>>,
}

impl IpcServer {
    pub fn new() -> Self {
        Self {
            registration_id: RefCell::new(None),
        }
    }

    pub fn register_object<F>(
        &self,
        connection: &DBusConnection,
        callback: F,
    ) -> Result<()>
    where
        F: Fn(IpcMessage) -> IpcResponse + 'static,
    {
        let interface_info = gio::DBusNodeInfo::for_xml(DBUS_INTERFACE_XML)
            .ok()
            .and_then(|e| e.lookup_interface(DBUS_INTERFACE_NAME))
            .context("Failed to parse DBus interface XML")?;

        let registration_id = connection
            .register_object(DBUS_OBJECT_PATH, &interface_info)
            .method_call(move |_connection, _sender, _path, _interface, method, params, invocation| {
                let callback = &callback;
                match IpcMessage::from_method_call(method, params) {
                    Ok(message) => {
                        let response = callback(message);
                        invocation.return_value(Some(&response.to_variant()));
                    }
                    Err(e) => {
                        invocation.return_gerror(e);
                    }
                }
            })
            .build()?;

        self.registration_id.replace(Some(registration_id));
        Ok(())
    }
}

pub struct IpcClient;

impl IpcClient {
    pub async fn send_message(message: &IpcMessage) -> Result<IpcResponse> {
        let connection = gio::bus_get_future(gio::BusType::Session)
            .await
            .context("Failed to connect to session bus")?;

        let (method_name, params): (&str, Variant) = match message {
            IpcMessage::LoadImage { filename } => ("LoadImage", (filename,).to_variant()),
            IpcMessage::Shutdown => ("Shutdown", ().to_variant()),
            IpcMessage::Ping => ("Ping", ().to_variant()),
        };

        let result = connection
            .call_future(
                Some("com.gabm.satty"),
                DBUS_OBJECT_PATH,
                DBUS_INTERFACE_NAME,
                method_name,
                Some(&params),
                None,
                gio::DBusCallFlags::NONE,
                5000,
            )
            .await
            .context("Failed to call DBus method (is the daemon running?)")?;

        let (response_str,): (String,) = result
            .get()
            .context("Failed to parse DBus response")?;

        if response_str.starts_with("Error:") {
            Ok(IpcResponse::Error(
                response_str.strip_prefix("Error: ").unwrap_or(&response_str).to_string()
            ))
        } else if response_str == "Pong" {
            Ok(IpcResponse::Pong)
        } else {
            Ok(IpcResponse::Ok)
        }
    }
}
