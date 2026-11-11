//! Commands sent over command and intercept connections.

use crate::code::Code;
use crate::types::{
    AccessLevel, CodeChannel, HttpEndpointType, InterceptionMode, LogLevel, MessageType,
    SessionType, SubscriptionMode,
};
use serde::Serialize;
use serde_json::Value;

/// A command. Serializes to a JSON object with a `"command"` member and camelCase fields.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "command", rename_all_fields = "camelCase")]
pub enum Command {
    /// Acknowledge a received object model (subscribe mode).
    Acknowledge,
    /// Retrieve the full object model.
    GetObjectModel,
    /// Lock the object model.
    LockObjectModel,
    /// Unlock the object model.
    UnlockObjectModel,
    /// Wait for the object model to be synchronized.
    SyncObjectModel,
    /// Apply a patch to the object model.
    PatchObjectModel {
        /// Key to patch.
        key: String,
        /// Patch value.
        patch: Value,
    },
    /// Set a property of the object model.
    SetObjectModel {
        /// Property path.
        property_path: String,
        /// New value.
        value: String,
    },
    /// Enable or disable a network protocol.
    SetNetworkProtocol {
        /// Protocol name.
        network_protocol: String,
        /// Whether it is enabled.
        enabled: bool,
    },
    /// Check a password (M551).
    CheckPassword {
        /// Password to check.
        password: String,
    },
    /// Evaluate an expression.
    EvaluateExpression {
        /// Channel context.
        channel: CodeChannel,
        /// Expression.
        expression: String,
    },
    /// Flush a code channel.
    Flush {
        /// Channel to flush.
        channel: CodeChannel,
        /// Synchronize file streams.
        sync_file_streams: bool,
        /// Only flush if the channel is executing.
        if_executing: bool,
    },
    /// Invalidate a code channel.
    InvalidateChannel {
        /// Channel to invalidate.
        channel: CodeChannel,
    },
    /// Override the update status.
    SetUpdateStatus {
        /// Whether an update is in progress.
        updating: bool,
    },
    /// Execute a code given as text.
    SimpleCode {
        /// The code.
        code: String,
        /// Destination channel.
        channel: CodeChannel,
        /// Allow asynchronous execution.
        execute_asynchronously: bool,
    },
    /// Write a message.
    WriteMessage {
        /// Message type.
        #[serde(rename = "type")]
        message_type: MessageType,
        /// Message content.
        content: String,
        /// Output the message on the console/object model.
        output_message: bool,
        /// Log level.
        log_level: Option<LogLevel>,
    },
    /// Parse a G-code file.
    GetFileInfo {
        /// File name.
        file_name: String,
        /// Whether to read thumbnail content.
        read_thumbnail_content: bool,
    },
    /// Resolve a RepRapFirmware path.
    ResolvePath {
        /// Path to resolve.
        path: String,
        /// Base directory.
        base_directory: Option<String>,
    },
    /// Add an HTTP endpoint.
    AddHttpEndpoint {
        /// Endpoint type.
        endpoint_type: HttpEndpointType,
        /// Namespace.
        namespace: String,
        /// Path.
        path: String,
        /// Whether this is an upload request.
        is_upload_request: bool,
    },
    /// Remove an HTTP endpoint.
    RemoveHttpEndpoint {
        /// Endpoint type.
        endpoint_type: HttpEndpointType,
        /// Namespace.
        namespace: String,
        /// Path.
        path: String,
    },
    /// Add a user session.
    AddUserSession {
        /// Access level.
        access_level: AccessLevel,
        /// Session type.
        session_type: SessionType,
        /// Origin.
        origin: String,
    },
    /// Remove a user session.
    RemoveUserSession {
        /// Session id.
        id: i64,
    },
    /// Install a plugin.
    InstallPlugin {
        /// Plugin ZIP file.
        plugin_file: String,
    },
    /// Reload a plugin manifest.
    ReloadPlugin {
        /// Plugin identifier.
        plugin: String,
    },
    /// Set plugin data.
    SetPluginData {
        /// Plugin identifier.
        plugin: String,
        /// Key.
        key: String,
        /// Value (any JSON).
        value: Value,
    },
    /// Start a plugin.
    StartPlugin {
        /// Plugin identifier.
        plugin: String,
        /// Save the state.
        save_state: bool,
    },
    /// Start all plugins.
    StartPlugins,
    /// Stop a plugin.
    StopPlugin {
        /// Plugin identifier.
        plugin: String,
        /// Save the state.
        save_state: bool,
    },
    /// Stop all plugins.
    StopPlugins,
    /// Uninstall a plugin.
    UninstallPlugin {
        /// Plugin identifier.
        plugin: String,
    },
    /// Install a system package.
    InstallSystemPackage {
        /// Package file.
        package_file: String,
    },
    /// Uninstall a system package.
    UninstallSystemPackage {
        /// Package identifier.
        package: String,
    },
    /// Cancel the intercepted code.
    Cancel,
    /// Ignore the intercepted code.
    Ignore,
    /// Resolve the intercepted code.
    Resolve {
        /// Message type.
        #[serde(rename = "type")]
        message_type: MessageType,
        /// Message content.
        content: Option<String>,
    },
    /// Execute a pre-parsed code.
    Code(Code),
}

/// Initialization message sent by the client right after the server hello.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "mode", rename_all_fields = "camelCase")]
pub enum InitMessage {
    /// Command mode.
    Command {
        /// Protocol version.
        version: u32,
    },
    /// Intercept mode.
    Intercept {
        /// Protocol version.
        version: u32,
        /// Interception mode.
        interception_mode: InterceptionMode,
        /// Channels to intercept codes on.
        channels: Vec<CodeChannel>,
        /// Code filters.
        filters: Option<Vec<String>>,
        /// Also intercept priority codes.
        priority_codes: bool,
        /// Flush automatically before notifying the client.
        auto_flush: bool,
        /// Evaluate expressions automatically.
        auto_evaluate_expressions: bool,
    },
    /// Subscribe mode.
    Subscribe {
        /// Protocol version.
        version: u32,
        /// Subscription mode.
        subscription_mode: SubscriptionMode,
        /// Legacy filter string.
        filter: String,
        /// Filter expressions.
        filters: Option<Vec<String>>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code_parameter::CodeParameter;
    use crate::types::CodeType;
    use crate::PROTOCOL_VERSION;
    use serde_json::json;

    fn j(c: &Command) -> Value {
        serde_json::to_value(c).unwrap()
    }

    #[test]
    fn simple_commands() {
        for (c, n) in [
            (Command::Acknowledge, "Acknowledge"),
            (Command::GetObjectModel, "GetObjectModel"),
            (Command::LockObjectModel, "LockObjectModel"),
            (Command::UnlockObjectModel, "UnlockObjectModel"),
            (Command::SyncObjectModel, "SyncObjectModel"),
            (Command::StartPlugins, "StartPlugins"),
            (Command::StopPlugins, "StopPlugins"),
            (Command::Cancel, "Cancel"),
            (Command::Ignore, "Ignore"),
        ] {
            assert_eq!(j(&c), json!({"command": n}));
        }
    }

    #[test]
    fn command_tag_comes_first() {
        let s = serde_json::to_string(&Command::Flush {
            channel: CodeChannel::Sbc,
            sync_file_streams: false,
            if_executing: true,
        })
        .unwrap();
        assert_eq!(
            s,
            r#"{"command":"Flush","channel":"SBC","syncFileStreams":false,"ifExecuting":true}"#
        );
    }

    #[test]
    fn parameterized_commands() {
        assert_eq!(
            j(&Command::PatchObjectModel {
                key: "k".into(),
                patch: json!({"a":1})
            }),
            json!({"command":"PatchObjectModel","key":"k","patch":{"a":1}})
        );
        assert_eq!(
            j(&Command::SetObjectModel {
                property_path: "a.b".into(),
                value: "v".into()
            }),
            json!({"command":"SetObjectModel","propertyPath":"a.b","value":"v"})
        );
        assert_eq!(
            j(&Command::SetNetworkProtocol {
                network_protocol: "HTTP".into(),
                enabled: true
            }),
            json!({"command":"SetNetworkProtocol","networkProtocol":"HTTP","enabled":true})
        );
        assert_eq!(
            j(&Command::CheckPassword {
                password: "p".into()
            }),
            json!({"command":"CheckPassword","password":"p"})
        );
        assert_eq!(
            j(&Command::EvaluateExpression {
                channel: CodeChannel::Http,
                expression: "1+1".into()
            }),
            json!({"command":"EvaluateExpression","channel":"HTTP","expression":"1+1"})
        );
        assert_eq!(
            j(&Command::InvalidateChannel {
                channel: CodeChannel::File
            }),
            json!({"command":"InvalidateChannel","channel":"File"})
        );
        assert_eq!(
            j(&Command::SetUpdateStatus { updating: true }),
            json!({"command":"SetUpdateStatus","updating":true})
        );
        assert_eq!(
            j(&Command::SimpleCode {
                code: "M115".into(),
                channel: CodeChannel::Sbc,
                execute_asynchronously: false
            }),
            json!({"command":"SimpleCode","code":"M115","channel":"SBC","executeAsynchronously":false})
        );
        assert_eq!(
            j(&Command::WriteMessage {
                message_type: MessageType::Warning,
                content: "c".into(),
                output_message: true,
                log_level: None
            }),
            json!({"command":"WriteMessage","type":1,"content":"c","outputMessage":true,"logLevel":null})
        );
        assert_eq!(
            j(&Command::WriteMessage {
                message_type: MessageType::Error,
                content: "c".into(),
                output_message: false,
                log_level: Some(LogLevel::Info)
            }),
            json!({"command":"WriteMessage","type":2,"content":"c","outputMessage":false,"logLevel":"info"})
        );
        assert_eq!(
            j(&Command::GetFileInfo {
                file_name: "f.g".into(),
                read_thumbnail_content: false
            }),
            json!({"command":"GetFileInfo","fileName":"f.g","readThumbnailContent":false})
        );
        assert_eq!(
            j(&Command::ResolvePath {
                path: "0:/gcodes".into(),
                base_directory: None
            }),
            json!({"command":"ResolvePath","path":"0:/gcodes","baseDirectory":null})
        );
        assert_eq!(
            j(&Command::AddHttpEndpoint {
                endpoint_type: HttpEndpointType::Get,
                namespace: "custom".into(),
                path: "getIt".into(),
                is_upload_request: false
            }),
            json!({"command":"AddHttpEndpoint","endpointType":"GET","namespace":"custom","path":"getIt","isUploadRequest":false})
        );
        assert_eq!(
            j(&Command::RemoveHttpEndpoint {
                endpoint_type: HttpEndpointType::WebSocket,
                namespace: "n".into(),
                path: "p".into()
            }),
            json!({"command":"RemoveHttpEndpoint","endpointType":"WebSocket","namespace":"n","path":"p"})
        );
        assert_eq!(
            j(&Command::AddUserSession {
                access_level: AccessLevel::ReadWrite,
                session_type: SessionType::Local,
                origin: "1".into()
            }),
            json!({"command":"AddUserSession","accessLevel":"readWrite","sessionType":"local","origin":"1"})
        );
        assert_eq!(
            j(&Command::RemoveUserSession { id: 3 }),
            json!({"command":"RemoveUserSession","id":3})
        );
        assert_eq!(
            j(&Command::InstallPlugin {
                plugin_file: "a.zip".into()
            }),
            json!({"command":"InstallPlugin","pluginFile":"a.zip"})
        );
        assert_eq!(
            j(&Command::ReloadPlugin { plugin: "p".into() }),
            json!({"command":"ReloadPlugin","plugin":"p"})
        );
        assert_eq!(
            j(&Command::SetPluginData {
                plugin: "p".into(),
                key: "k".into(),
                value: json!([1, 2])
            }),
            json!({"command":"SetPluginData","plugin":"p","key":"k","value":[1,2]})
        );
        assert_eq!(
            j(&Command::StartPlugin {
                plugin: "p".into(),
                save_state: true
            }),
            json!({"command":"StartPlugin","plugin":"p","saveState":true})
        );
        assert_eq!(
            j(&Command::StopPlugin {
                plugin: "p".into(),
                save_state: false
            }),
            json!({"command":"StopPlugin","plugin":"p","saveState":false})
        );
        assert_eq!(
            j(&Command::UninstallPlugin { plugin: "p".into() }),
            json!({"command":"UninstallPlugin","plugin":"p"})
        );
        assert_eq!(
            j(&Command::InstallSystemPackage {
                package_file: "x.deb".into()
            }),
            json!({"command":"InstallSystemPackage","packageFile":"x.deb"})
        );
        assert_eq!(
            j(&Command::UninstallSystemPackage {
                package: "x".into()
            }),
            json!({"command":"UninstallSystemPackage","package":"x"})
        );
        assert_eq!(
            j(&Command::Resolve {
                message_type: MessageType::Success,
                content: None
            }),
            json!({"command":"Resolve","type":0,"content":null})
        );
        assert_eq!(
            j(&Command::Resolve {
                message_type: MessageType::Warning,
                content: Some("hi".into())
            }),
            json!({"command":"Resolve","type":1,"content":"hi"})
        );
    }

    #[test]
    fn code_command() {
        let code = Code {
            code_type: CodeType::GCode,
            major_number: Some(28),
            parameters: vec![CodeParameter::new('X', "0", false, false)],
            ..Code::default()
        };
        let v = j(&Command::Code(code));
        assert_eq!(v["command"], "Code");
        assert_eq!(v["type"], "G");
        assert_eq!(v["channel"], "SBC");
        assert_eq!(v["majorNumber"], 28);
        assert_eq!(v["minorNumber"], Value::Null);
        assert_eq!(
            v["parameters"][0],
            json!({"letter":"X","value":"0","isString":false,"isDriverId":false})
        );
    }

    #[test]
    fn init_messages() {
        assert_eq!(
            serde_json::to_string(&InitMessage::Command {
                version: PROTOCOL_VERSION
            })
            .unwrap(),
            r#"{"mode":"Command","version":12}"#
        );
        assert_eq!(
            serde_json::to_value(InitMessage::Intercept {
                version: 12,
                interception_mode: InterceptionMode::Pre,
                channels: vec![CodeChannel::Sbc, CodeChannel::File],
                filters: Some(vec!["M1234".into()]),
                priority_codes: false,
                auto_flush: true,
                auto_evaluate_expressions: true,
            })
            .unwrap(),
            json!({"mode":"Intercept","version":12,"interceptionMode":"Pre","channels":["SBC","File"],
                   "filters":["M1234"],"priorityCodes":false,"autoFlush":true,"autoEvaluateExpressions":true})
        );
        assert_eq!(
            serde_json::to_value(InitMessage::Subscribe {
                version: 12,
                subscription_mode: SubscriptionMode::Patch,
                filter: String::new(),
                filters: None,
            })
            .unwrap(),
            json!({"mode":"Subscribe","version":12,"subscriptionMode":"Patch","filter":"","filters":null})
        );
    }
}
