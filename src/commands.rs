use std::fmt;

use crate::parser::RespValue;

#[derive(Debug)]
pub enum Command {
    Ping(Option<Vec<u8>>),
    Echo(Vec<u8>),
}

#[derive(Debug)]
pub enum CommandError {
    NotAnArray,
    EmptyCommand,
    CommandNotBulkString,
    UnknownCommand(String),
    WrongArgCount { command: String, expected: String },
    InvalidArgument { command: String, reason: String },
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommandError::NotAnArray => write!(f, "Command must be an array of bulk strings"),
            CommandError::EmptyCommand => write!(f, "Empty command"),
            CommandError::CommandNotBulkString => write!(f, "Command name must be a bulk string"),
            CommandError::UnknownCommand(cmd) => write!(f, "unknown command `{}`", cmd),
            CommandError::WrongArgCount { command, expected } => {
                write!(
                    f,
                    "wrong number of arguments for '{}' command. Expected {}",
                    command, expected
                )
            }
            CommandError::InvalidArgument { command, reason } => {
                write!(f, "invalid argument for '{}' command: {}", command, reason)
            }
        }
    }
}

impl std::error::Error for CommandError {}

impl Command {
    pub fn from_resp(resp: RespValue) -> Result<Self, CommandError> {
        let RespValue::Array(elements) = resp else {
            return Err(CommandError::NotAnArray);
        };
        if elements.is_empty() {
            return Err(CommandError::EmptyCommand);
        }
        let mut args = &elements[..];

        let Some(RespValue::BulkString(command_bytes)) = args.first() else {
            return Err(CommandError::CommandNotBulkString);
        };

        let cmd_name = String::from_utf8_lossy(command_bytes).to_ascii_uppercase();
        args = &args[1..];

        match cmd_name.as_str() {
            "PING" => {
                if args.is_empty() {
                    Ok(Command::Ping(None))
                } else if let Some(RespValue::BulkString(msg)) = args.first() {
                    Ok(Command::Ping(Some(msg.clone())))
                } else {
                    Err(CommandError::InvalidArgument {
                        command: "PING".to_string(),
                        reason: "Argument must be a bulk string".to_string(),
                    })
                }
            }
            "ECHO" => {
                if args.len() != 1 {
                    return Err(CommandError::WrongArgCount {
                        command: "ECHO".to_string(),
                        expected: "1".to_string(),
                    });
                }
                if let Some(RespValue::BulkString(msg)) = args.first() {
                    Ok(Command::Echo(msg.clone()))
                } else {
                    Err(CommandError::InvalidArgument {
                        command: "ECHO".to_string(),
                        reason: "argument must be a bulk string".to_string(),
                    })
                }
            }
            _ => Err(CommandError::UnknownCommand(cmd_name)),
        }
    }

    pub fn execute(&self) -> Vec<u8> {
        match self {
            Command::Ping(None) => b"+PONG\r\n".to_vec(),
            Command::Ping(Some(msg)) | Command::Echo(msg) => {
                format!("${}\r\n{}\r\n", msg.len(), String::from_utf8_lossy(&msg)).into_bytes()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::commands::{Command, CommandError};
    use crate::parser::RespValue;

    #[test]
    fn test_ping_command() {
        // Given
        let resp_value = RespValue::Array(vec![RespValue::BulkString(b"PING".to_vec())]);

        // Act
        let command = Command::from_resp(resp_value).unwrap();
        let response = command.execute();

        // Assert
        assert_eq!(response, b"+PONG\r\n");
    }

    fn echo_test(resp: RespValue) {
        // Echo can be echoed with PING or ECHO commands
        // Given
        let argument = b"hello";
        let resp_value = RespValue::Array(vec![resp, RespValue::BulkString(argument.to_vec())]);

        // Act
        let command = Command::from_resp(resp_value).unwrap();
        let response = command.execute();

        // Assert
        let expected_response = format!(
            "${}\r\n{}\r\n",
            argument.len(),
            String::from_utf8_lossy(argument)
        );
        assert_eq!(response, expected_response.into_bytes());
    }

    #[test]
    fn test_ping_command_argument() {
        echo_test(RespValue::BulkString(b"PING".to_vec()));
        echo_test(RespValue::BulkString(b"ping".to_vec()));
    }

    #[test]
    fn test_echo_command() {
        echo_test(RespValue::BulkString(b"eChO".to_vec()));
        echo_test(RespValue::BulkString(b"echo".to_vec()));
    }
}

// pub fn process_value_into_command(resp: RespValue) {
//     match resp {
//         RespValue::SimpleString(_) => todo!(),
//         RespValue::Integer(_) => todo!(),
//         RespValue::BulkString(bytes) => handle_command(),
//         RespValue::Array(elements) => {
//             for element in elements {
//                 process_value_into_command(element);
//             }
//         }
//         RespValue::Null => todo!(),
//     }
// }
