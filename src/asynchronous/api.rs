use super::base::AsyncBaseConnection;
use super::http::AsyncHttpEndpointUnixSocket;
use crate::code::Code;
use crate::commands::Command;
use crate::connection::{code_result, endpoint_socket_path, result_as, session_id};
use crate::error::Result;
use crate::types::{
    AccessLevel, CodeChannel, HttpEndpointType, LogLevel, Message, MessageType, SessionType,
};
use crate::DEFAULT_BACKLOG;
use serde_json::Value;
use std::future::Future;

/// Async version of [`CommandApi`](crate::CommandApi).
///
/// Implementors only provide access to the underlying [`AsyncBaseConnection`]. All returned
/// futures are `Send`. These calls are request/response exchanges and must not be cancelled
/// (dropped) midway, otherwise the connection may be left out of sync.
pub trait AsyncCommandApi: Send {
    /// The underlying base connection.
    fn connection_mut(&mut self) -> &mut AsyncBaseConnection;

    /// Perform an arbitrary command and return its result (`null` if none).
    fn perform_command(&mut self, command: &Command) -> impl Future<Output = Result<Value>> + Send {
        async move { self.connection_mut().perform_command(command).await }
    }

    /// Add a third-party HTTP endpoint (`/machine/{namespace}/{path}`) and start listening on it.
    ///
    /// Must be called from within a tokio runtime.
    fn add_http_endpoint(
        &mut self,
        endpoint_type: HttpEndpointType,
        namespace: &str,
        path: &str,
        is_upload_request: bool,
        backlog: u32,
    ) -> impl Future<Output = Result<AsyncHttpEndpointUnixSocket>> + Send {
        async move {
            let res = self
                .perform_command(&Command::AddHttpEndpoint {
                    endpoint_type,
                    namespace: namespace.into(),
                    path: path.into(),
                    is_upload_request,
                })
                .await?;
            let socket_file = endpoint_socket_path(&res)?;
            let debug = self.connection_mut().debug();
            AsyncHttpEndpointUnixSocket::new(
                endpoint_type,
                namespace,
                path,
                socket_file,
                backlog,
                debug,
            )
        }
    }

    /// Like [`add_http_endpoint`](Self::add_http_endpoint) with the default backlog and no upload.
    fn add_http_endpoint_default(
        &mut self,
        endpoint_type: HttpEndpointType,
        namespace: &str,
        path: &str,
    ) -> impl Future<Output = Result<AsyncHttpEndpointUnixSocket>> + Send {
        self.add_http_endpoint(endpoint_type, namespace, path, false, DEFAULT_BACKLOG)
    }

    /// Add a user session. `origin` defaults to the process id. Returns the session id.
    fn add_user_session(
        &mut self,
        access_level: AccessLevel,
        session_type: SessionType,
        origin: Option<&str>,
    ) -> impl Future<Output = Result<i64>> + Send {
        async move {
            let origin = origin
                .map(String::from)
                .unwrap_or_else(|| std::process::id().to_string());
            let res = self
                .perform_command(&Command::AddUserSession {
                    access_level,
                    session_type,
                    origin,
                })
                .await?;
            session_id(&res)
        }
    }

    /// Check a password (see M551).
    fn check_password(&mut self, password: &str) -> impl Future<Output = Result<bool>> + Send {
        async move {
            result_as(
                self.perform_command(&Command::CheckPassword {
                    password: password.into(),
                })
                .await?,
            )
        }
    }

    /// Evaluate an expression on a channel.
    fn evaluate_expression(
        &mut self,
        expression: &str,
        channel: CodeChannel,
    ) -> impl Future<Output = Result<Value>> + Send {
        async move {
            self.perform_command(&Command::EvaluateExpression {
                channel,
                expression: expression.into(),
            })
            .await
        }
    }

    /// Wait for all pending codes of the channel to finish.
    fn flush(&mut self, channel: CodeChannel) -> impl Future<Output = Result<bool>> + Send {
        self.flush_with(channel, false, true)
    }

    /// [`flush`](Self::flush) with explicit options.
    fn flush_with(
        &mut self,
        channel: CodeChannel,
        sync_file_streams: bool,
        if_executing: bool,
    ) -> impl Future<Output = Result<bool>> + Send {
        async move {
            result_as(
                self.perform_command(&Command::Flush {
                    channel,
                    sync_file_streams,
                    if_executing,
                })
                .await?,
            )
        }
    }

    /// Parse a G-code file and return its file information.
    fn get_file_info(
        &mut self,
        file_name: &str,
        read_thumbnail_content: bool,
    ) -> impl Future<Output = Result<Value>> + Send {
        async move {
            self.perform_command(&Command::GetFileInfo {
                file_name: file_name.into(),
                read_thumbnail_content,
            })
            .await
        }
    }

    /// Retrieve the full object model.
    fn get_object_model(&mut self) -> impl Future<Output = Result<Value>> + Send {
        async move { self.perform_command(&Command::GetObjectModel).await }
    }

    /// Retrieve the full object model as raw JSON text of the server's reply.
    fn get_serialized_object_model(&mut self) -> impl Future<Output = Result<String>> + Send {
        async move {
            let conn = self.connection_mut();
            conn.send(&Command::GetObjectModel).await?;
            conn.receive_json().await
        }
    }

    /// Install or upgrade a plugin.
    fn install_plugin(&mut self, plugin_file: &str) -> impl Future<Output = Result<()>> + Send {
        async move {
            self.perform_command(&Command::InstallPlugin {
                plugin_file: plugin_file.into(),
            })
            .await
            .map(drop)
        }
    }

    /// Install or upgrade a system package (absolute path).
    fn install_system_package(
        &mut self,
        package_file: &str,
    ) -> impl Future<Output = Result<()>> + Send {
        async move {
            self.perform_command(&Command::InstallSystemPackage {
                package_file: package_file.into(),
            })
            .await
            .map(drop)
        }
    }

    /// Invalidate all pending codes and files on a channel.
    fn invalidate_channel(
        &mut self,
        channel: CodeChannel,
    ) -> impl Future<Output = Result<bool>> + Send {
        async move {
            result_as(
                self.perform_command(&Command::InvalidateChannel { channel })
                    .await?,
            )
        }
    }

    /// Lock the object model. [`unlock_object_model`](Self::unlock_object_model) is mandatory afterwards.
    fn lock_object_model(&mut self) -> impl Future<Output = Result<()>> + Send {
        async move {
            self.perform_command(&Command::LockObjectModel)
                .await
                .map(drop)
        }
    }

    /// Apply a full patch to the object model. Use with care.
    fn patch_object_model(
        &mut self,
        key: &str,
        patch: &Value,
    ) -> impl Future<Output = Result<()>> + Send {
        async move {
            self.perform_command(&Command::PatchObjectModel {
                key: key.into(),
                patch: patch.clone(),
            })
            .await
            .map(drop)
        }
    }

    /// Execute a pre-parsed code and return its resulting messages.
    fn perform_code(
        &mut self,
        code: &Code,
    ) -> impl Future<Output = Result<Option<Vec<Message>>>> + Send {
        async move { code_result(self.perform_command(&Command::Code(code.clone())).await?) }
    }

    /// Execute a G/M/T-code given as text and return its reply.
    fn perform_simple_code(
        &mut self,
        code: &str,
        channel: CodeChannel,
        async_exec: bool,
    ) -> impl Future<Output = Result<String>> + Send {
        async move {
            result_as(
                self.perform_command(&Command::SimpleCode {
                    code: code.into(),
                    channel,
                    execute_asynchronously: async_exec,
                })
                .await?,
            )
        }
    }

    /// Reload the manifest of a plugin.
    fn reload_plugin(&mut self, plugin: &str) -> impl Future<Output = Result<()>> + Send {
        async move {
            self.perform_command(&Command::ReloadPlugin {
                plugin: plugin.into(),
            })
            .await
            .map(drop)
        }
    }

    /// Remove an HTTP endpoint.
    fn remove_http_endpoint(
        &mut self,
        endpoint_type: HttpEndpointType,
        namespace: &str,
        path: &str,
    ) -> impl Future<Output = Result<bool>> + Send {
        async move {
            result_as(
                self.perform_command(&Command::RemoveHttpEndpoint {
                    endpoint_type,
                    namespace: namespace.into(),
                    path: path.into(),
                })
                .await?,
            )
        }
    }

    /// Remove a user session.
    fn remove_user_session(&mut self, id: i64) -> impl Future<Output = Result<bool>> + Send {
        async move {
            result_as(
                self.perform_command(&Command::RemoveUserSession { id })
                    .await?,
            )
        }
    }

    /// Resolve a RepRapFirmware-style path to a real file path.
    fn resolve_path(&mut self, path: &str) -> impl Future<Output = Result<String>> + Send {
        async move {
            result_as(
                self.perform_command(&Command::ResolvePath {
                    path: path.into(),
                    base_directory: None,
                })
                .await?,
            )
        }
    }

    /// Enable or disable a network protocol (lock the object model first).
    fn set_network_protocol(
        &mut self,
        protocol: &str,
        enabled: bool,
    ) -> impl Future<Output = Result<()>> + Send {
        async move {
            self.perform_command(&Command::SetNetworkProtocol {
                network_protocol: protocol.into(),
                enabled,
            })
            .await
            .map(drop)
        }
    }

    /// Set a property of the object model (lock the object model first).
    fn set_object_model(
        &mut self,
        path: &str,
        value: &str,
    ) -> impl Future<Output = Result<()>> + Send {
        async move {
            self.perform_command(&Command::SetObjectModel {
                property_path: path.into(),
                value: value.into(),
            })
            .await
            .map(drop)
        }
    }

    /// Set custom plugin data in the object model.
    fn set_plugin_data(
        &mut self,
        plugin: &str,
        key: &str,
        value: impl Into<Value>,
    ) -> impl Future<Output = Result<()>> + Send {
        let value = value.into();
        async move {
            self.perform_command(&Command::SetPluginData {
                plugin: plugin.into(),
                key: key.into(),
                value,
            })
            .await
            .map(drop)
        }
    }

    /// Override the machine status while a software update is in progress.
    fn set_update_status(&mut self, updating: bool) -> impl Future<Output = Result<()>> + Send {
        async move {
            self.perform_command(&Command::SetUpdateStatus { updating })
                .await
                .map(drop)
        }
    }

    /// Start a plugin.
    fn start_plugin(
        &mut self,
        plugin: &str,
        save_state: bool,
    ) -> impl Future<Output = Result<()>> + Send {
        async move {
            self.perform_command(&Command::StartPlugin {
                plugin: plugin.into(),
                save_state,
            })
            .await
            .map(drop)
        }
    }

    /// Start all previously started plugins again.
    fn start_plugins(&mut self) -> impl Future<Output = Result<()>> + Send {
        async move { self.perform_command(&Command::StartPlugins).await.map(drop) }
    }

    /// Stop a plugin.
    fn stop_plugin(
        &mut self,
        plugin: &str,
        save_state: bool,
    ) -> impl Future<Output = Result<()>> + Send {
        async move {
            self.perform_command(&Command::StopPlugin {
                plugin: plugin.into(),
                save_state,
            })
            .await
            .map(drop)
        }
    }

    /// Stop all plugins.
    fn stop_plugins(&mut self) -> impl Future<Output = Result<()>> + Send {
        async move { self.perform_command(&Command::StopPlugins).await.map(drop) }
    }

    /// Wait for the full object model to be updated from RepRapFirmware.
    fn sync_object_model(&mut self) -> impl Future<Output = Result<()>> + Send {
        async move {
            self.perform_command(&Command::SyncObjectModel)
                .await
                .map(drop)
        }
    }

    /// Uninstall a plugin.
    fn uninstall_plugin(&mut self, plugin: &str) -> impl Future<Output = Result<()>> + Send {
        async move {
            self.perform_command(&Command::UninstallPlugin {
                plugin: plugin.into(),
            })
            .await
            .map(drop)
        }
    }

    /// Uninstall a system package.
    fn uninstall_system_package(
        &mut self,
        package: &str,
    ) -> impl Future<Output = Result<()>> + Send {
        async move {
            self.perform_command(&Command::UninstallSystemPackage {
                package: package.into(),
            })
            .await
            .map(drop)
        }
    }

    /// Unlock the object model again.
    fn unlock_object_model(&mut self) -> impl Future<Output = Result<()>> + Send {
        async move {
            self.perform_command(&Command::UnlockObjectModel)
                .await
                .map(drop)
        }
    }

    /// Write an arbitrary message.
    fn write_message(
        &mut self,
        message_type: MessageType,
        content: &str,
        output_message: bool,
        log_level: Option<LogLevel>,
    ) -> impl Future<Output = Result<()>> + Send {
        async move {
            self.perform_command(&Command::WriteMessage {
                message_type,
                content: content.into(),
                output_message,
                log_level,
            })
            .await
            .map(drop)
        }
    }
}
