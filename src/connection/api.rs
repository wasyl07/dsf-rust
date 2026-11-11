use super::base::BaseConnection;
use crate::code::Code;
use crate::commands::Command;
use crate::error::{Error, Result};
use crate::http::HttpEndpointUnixSocket;
use crate::types::{
    AccessLevel, CodeChannel, HttpEndpointType, LogLevel, Message, MessageType, SessionType,
};
use crate::DEFAULT_BACKLOG;
use serde::de::DeserializeOwned;
use serde_json::Value;

/// Convert a command result into `T`, treating `null` as `T::default()`.
pub(crate) fn result_as<T: DeserializeOwned + Default>(v: Value) -> Result<T> {
    if v.is_null() {
        Ok(T::default())
    } else {
        Ok(serde_json::from_value(v)?)
    }
}

/// Convert the result of `Code` into messages (null, a message list or a single message).
pub(crate) fn code_result(v: Value) -> Result<Option<Vec<Message>>> {
    match v {
        Value::Null => Ok(None),
        v @ Value::Array(_) => Ok(Some(serde_json::from_value(v)?)),
        v => Ok(Some(vec![serde_json::from_value(v)?])),
    }
}

/// Extract the session id from the result of `AddUserSession`.
pub(crate) fn session_id(res: &Value) -> Result<i64> {
    res.as_i64()
        .or_else(|| res.as_str().and_then(|s| s.parse().ok()))
        .ok_or_else(|| Error::InvalidArgument("server did not return a session id".into()))
}

/// Extract the socket path from the result of `AddHttpEndpoint`.
pub(crate) fn endpoint_socket_path(res: &Value) -> Result<&str> {
    res.as_str().ok_or_else(|| {
        Error::InvalidArgument("server did not return an endpoint socket path".into())
    })
}

/// The command API shared by command and intercept connections.
///
/// Implementors only provide access to the underlying [`BaseConnection`].
pub trait CommandApi {
    /// The underlying base connection.
    fn connection_mut(&mut self) -> &mut BaseConnection;

    /// Perform an arbitrary command and return its result (`null` if none).
    fn perform_command(&mut self, command: &Command) -> Result<Value> {
        self.connection_mut().perform_command(command)
    }

    /// Add a third-party HTTP endpoint (`/machine/{namespace}/{path}`) and start listening on it.
    fn add_http_endpoint(
        &mut self,
        endpoint_type: HttpEndpointType,
        namespace: &str,
        path: &str,
        is_upload_request: bool,
        backlog: u32,
    ) -> Result<HttpEndpointUnixSocket> {
        let res = self.perform_command(&Command::AddHttpEndpoint {
            endpoint_type,
            namespace: namespace.into(),
            path: path.into(),
            is_upload_request,
        })?;
        let socket_file = endpoint_socket_path(&res)?;
        let debug = self.connection_mut().debug();
        HttpEndpointUnixSocket::new(endpoint_type, namespace, path, socket_file, backlog, debug)
    }

    /// Like [`add_http_endpoint`](Self::add_http_endpoint) with the default backlog and no upload.
    fn add_http_endpoint_default(
        &mut self,
        endpoint_type: HttpEndpointType,
        namespace: &str,
        path: &str,
    ) -> Result<HttpEndpointUnixSocket> {
        self.add_http_endpoint(endpoint_type, namespace, path, false, DEFAULT_BACKLOG)
    }

    /// Add a user session. `origin` defaults to the process id. Returns the session id.
    fn add_user_session(
        &mut self,
        access_level: AccessLevel,
        session_type: SessionType,
        origin: Option<&str>,
    ) -> Result<i64> {
        let origin = origin
            .map(String::from)
            .unwrap_or_else(|| std::process::id().to_string());
        let res = self.perform_command(&Command::AddUserSession {
            access_level,
            session_type,
            origin,
        })?;
        session_id(&res)
    }

    /// Check a password (see M551).
    fn check_password(&mut self, password: &str) -> Result<bool> {
        result_as(self.perform_command(&Command::CheckPassword {
            password: password.into(),
        })?)
    }

    /// Evaluate an expression on a channel.
    fn evaluate_expression(&mut self, expression: &str, channel: CodeChannel) -> Result<Value> {
        self.perform_command(&Command::EvaluateExpression {
            channel,
            expression: expression.into(),
        })
    }

    /// Wait for all pending codes of the channel to finish.
    fn flush(&mut self, channel: CodeChannel) -> Result<bool> {
        self.flush_with(channel, false, true)
    }

    /// [`flush`](Self::flush) with explicit options.
    fn flush_with(
        &mut self,
        channel: CodeChannel,
        sync_file_streams: bool,
        if_executing: bool,
    ) -> Result<bool> {
        result_as(self.perform_command(&Command::Flush {
            channel,
            sync_file_streams,
            if_executing,
        })?)
    }

    /// Parse a G-code file and return its file information.
    fn get_file_info(&mut self, file_name: &str, read_thumbnail_content: bool) -> Result<Value> {
        self.perform_command(&Command::GetFileInfo {
            file_name: file_name.into(),
            read_thumbnail_content,
        })
    }

    /// Retrieve the full object model.
    fn get_object_model(&mut self) -> Result<Value> {
        self.perform_command(&Command::GetObjectModel)
    }

    /// Retrieve the full object model as raw JSON text of the server's reply.
    fn get_serialized_object_model(&mut self) -> Result<String> {
        let conn = self.connection_mut();
        conn.send(&Command::GetObjectModel)?;
        conn.receive_json()
    }

    /// Install or upgrade a plugin.
    fn install_plugin(&mut self, plugin_file: &str) -> Result<()> {
        self.perform_command(&Command::InstallPlugin {
            plugin_file: plugin_file.into(),
        })
        .map(drop)
    }

    /// Install or upgrade a system package (absolute path).
    fn install_system_package(&mut self, package_file: &str) -> Result<()> {
        self.perform_command(&Command::InstallSystemPackage {
            package_file: package_file.into(),
        })
        .map(drop)
    }

    /// Invalidate all pending codes and files on a channel.
    fn invalidate_channel(&mut self, channel: CodeChannel) -> Result<bool> {
        result_as(self.perform_command(&Command::InvalidateChannel { channel })?)
    }

    /// Lock the object model. [`unlock_object_model`](Self::unlock_object_model) is mandatory afterwards.
    fn lock_object_model(&mut self) -> Result<()> {
        self.perform_command(&Command::LockObjectModel).map(drop)
    }

    /// Apply a full patch to the object model. Use with care.
    fn patch_object_model(&mut self, key: &str, patch: &Value) -> Result<()> {
        self.perform_command(&Command::PatchObjectModel {
            key: key.into(),
            patch: patch.clone(),
        })
        .map(drop)
    }

    /// Execute a pre-parsed code and return its resulting messages.
    fn perform_code(&mut self, code: &Code) -> Result<Option<Vec<Message>>> {
        code_result(self.perform_command(&Command::Code(code.clone()))?)
    }

    /// Execute a G/M/T-code given as text and return its reply.
    fn perform_simple_code(
        &mut self,
        code: &str,
        channel: CodeChannel,
        async_exec: bool,
    ) -> Result<String> {
        result_as(self.perform_command(&Command::SimpleCode {
            code: code.into(),
            channel,
            execute_asynchronously: async_exec,
        })?)
    }

    /// Reload the manifest of a plugin.
    fn reload_plugin(&mut self, plugin: &str) -> Result<()> {
        self.perform_command(&Command::ReloadPlugin {
            plugin: plugin.into(),
        })
        .map(drop)
    }

    /// Remove an HTTP endpoint.
    fn remove_http_endpoint(
        &mut self,
        endpoint_type: HttpEndpointType,
        namespace: &str,
        path: &str,
    ) -> Result<bool> {
        result_as(self.perform_command(&Command::RemoveHttpEndpoint {
            endpoint_type,
            namespace: namespace.into(),
            path: path.into(),
        })?)
    }

    /// Remove a user session.
    fn remove_user_session(&mut self, id: i64) -> Result<bool> {
        result_as(self.perform_command(&Command::RemoveUserSession { id })?)
    }

    /// Resolve a RepRapFirmware-style path to a real file path.
    fn resolve_path(&mut self, path: &str) -> Result<String> {
        result_as(self.perform_command(&Command::ResolvePath {
            path: path.into(),
            base_directory: None,
        })?)
    }

    /// Enable or disable a network protocol (lock the object model first).
    fn set_network_protocol(&mut self, protocol: &str, enabled: bool) -> Result<()> {
        self.perform_command(&Command::SetNetworkProtocol {
            network_protocol: protocol.into(),
            enabled,
        })
        .map(drop)
    }

    /// Set a property of the object model (lock the object model first).
    fn set_object_model(&mut self, path: &str, value: &str) -> Result<()> {
        self.perform_command(&Command::SetObjectModel {
            property_path: path.into(),
            value: value.into(),
        })
        .map(drop)
    }

    /// Set custom plugin data in the object model.
    fn set_plugin_data(&mut self, plugin: &str, key: &str, value: impl Into<Value>) -> Result<()> {
        self.perform_command(&Command::SetPluginData {
            plugin: plugin.into(),
            key: key.into(),
            value: value.into(),
        })
        .map(drop)
    }

    /// Override the machine status while a software update is in progress.
    fn set_update_status(&mut self, updating: bool) -> Result<()> {
        self.perform_command(&Command::SetUpdateStatus { updating })
            .map(drop)
    }

    /// Start a plugin.
    fn start_plugin(&mut self, plugin: &str, save_state: bool) -> Result<()> {
        self.perform_command(&Command::StartPlugin {
            plugin: plugin.into(),
            save_state,
        })
        .map(drop)
    }

    /// Start all previously started plugins again.
    fn start_plugins(&mut self) -> Result<()> {
        self.perform_command(&Command::StartPlugins).map(drop)
    }

    /// Stop a plugin.
    fn stop_plugin(&mut self, plugin: &str, save_state: bool) -> Result<()> {
        self.perform_command(&Command::StopPlugin {
            plugin: plugin.into(),
            save_state,
        })
        .map(drop)
    }

    /// Stop all plugins.
    fn stop_plugins(&mut self) -> Result<()> {
        self.perform_command(&Command::StopPlugins).map(drop)
    }

    /// Wait for the full object model to be updated from RepRapFirmware.
    fn sync_object_model(&mut self) -> Result<()> {
        self.perform_command(&Command::SyncObjectModel).map(drop)
    }

    /// Uninstall a plugin.
    fn uninstall_plugin(&mut self, plugin: &str) -> Result<()> {
        self.perform_command(&Command::UninstallPlugin {
            plugin: plugin.into(),
        })
        .map(drop)
    }

    /// Uninstall a system package.
    fn uninstall_system_package(&mut self, package: &str) -> Result<()> {
        self.perform_command(&Command::UninstallSystemPackage {
            package: package.into(),
        })
        .map(drop)
    }

    /// Unlock the object model again.
    fn unlock_object_model(&mut self) -> Result<()> {
        self.perform_command(&Command::UnlockObjectModel).map(drop)
    }

    /// Write an arbitrary message.
    fn write_message(
        &mut self,
        message_type: MessageType,
        content: &str,
        output_message: bool,
        log_level: Option<LogLevel>,
    ) -> Result<()> {
        self.perform_command(&Command::WriteMessage {
            message_type,
            content: content.into(),
            output_message,
            log_level,
        })
        .map(drop)
    }
}
