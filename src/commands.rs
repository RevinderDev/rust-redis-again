use std::fmt;
use std::iter::Peekable;
use std::slice::Iter;
use std::str;
use std::time::{Duration, Instant};

use crate::db::{Database, DbValue};
use crate::parser::RespValue;

#[derive(Debug, PartialEq)]
pub enum CommandError {
    NotAnArray,
    EmptyCommand,
    CommandNotBulkString,
    UnknownCommand(String),
    WrongArgCount,
    InvalidArgument { reason: String },
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommandError::NotAnArray => write!(f, "Command must be an array of bulk strings"),
            CommandError::EmptyCommand => write!(f, "Empty command"),
            CommandError::CommandNotBulkString => write!(f, "Command name must be a bulk string"),
            CommandError::UnknownCommand(cmd) => write!(f, "unknown command `{}`", cmd),
            CommandError::WrongArgCount => write!(f, "wrong number of arguments"),
            CommandError::InvalidArgument { reason } => write!(f, "invalid argument: {}", reason),
        }
    }
}

impl std::error::Error for CommandError {}

struct ArgParser<'a> {
    iter: Peekable<Iter<'a, RespValue>>,
}

impl<'a> ArgParser<'a> {
    fn new(args: &'a [RespValue]) -> Self {
        Self {
            iter: args.iter().peekable(),
        }
    }

    fn next_bulk_string(&mut self) -> Result<Vec<u8>, CommandError> {
        match self.iter.next() {
            Some(RespValue::BulkString(bs)) => Ok(bs.clone()),
            Some(_) => Err(CommandError::InvalidArgument {
                reason: "argument must be a bulk string".to_string(),
            }),
            None => Err(CommandError::WrongArgCount),
        }
    }

    fn finish(&mut self) -> Result<(), CommandError> {
        if self.iter.peek().is_some() {
            Err(CommandError::WrongArgCount)
        } else {
            Ok(())
        }
    }
}

trait CommandExt {
    fn parse(parser: &mut ArgParser) -> Result<Self, CommandError>
    where
        Self: Sized;
    fn execute(self: Box<Self>, db: &Database) -> Vec<u8>;
}

#[derive(Debug, PartialEq)]
struct Ping {
    msg: Option<Vec<u8>>,
}

impl CommandExt for Ping {
    fn parse(parser: &mut ArgParser) -> Result<Self, CommandError> {
        let msg = match parser.iter.next() {
            Some(RespValue::BulkString(bs)) => Some(bs.clone()),
            Some(_) => {
                return Err(CommandError::InvalidArgument {
                    reason: "PING argument must be a bulk string".to_string(),
                })
            }
            None => None,
        };
        parser.finish()?;
        Ok(Ping { msg })
    }

    fn execute(self: Box<Self>, _db: &Database) -> Vec<u8> {
        match self.msg {
            Some(msg) => {
                format!("${}\r\n{}\r\n", msg.len(), String::from_utf8_lossy(&msg)).into_bytes()
            }
            None => b"+PONG\r\n".to_vec(),
        }
    }
}

#[derive(Debug, PartialEq)]
struct Echo {
    msg: Vec<u8>,
}

impl CommandExt for Echo {
    fn parse(parser: &mut ArgParser) -> Result<Self, CommandError> {
        let msg = parser.next_bulk_string()?;
        parser.finish()?;
        Ok(Echo { msg })
    }

    fn execute(self: Box<Self>, _db: &Database) -> Vec<u8> {
        format!(
            "${}\r\n{}\r\n",
            self.msg.len(),
            String::from_utf8_lossy(&self.msg)
        )
        .into_bytes()
    }
}

#[derive(Debug, PartialEq)]
struct Get {
    key: Vec<u8>,
}

impl CommandExt for Get {
    fn parse(parser: &mut ArgParser) -> Result<Self, CommandError> {
        let key = parser.next_bulk_string()?;
        parser.finish()?;
        Ok(Get { key })
    }

    fn execute(self: Box<Self>, db: &Database) -> Vec<u8> {
        let mut db_lock = db.lock().unwrap();
        let key_string = String::from_utf8_lossy(&self.key).to_ascii_uppercase();

        if let Some(db_value) = db_lock.get(&key_string) {
            if let Some(expires_at) = db_value.expires_at {
                if Instant::now() >= expires_at {
                    db_lock.remove(&key_string);
                    return b"$-1\r\n".to_vec();
                }
            }
            let string_value = String::from_utf8_lossy(&db_value.value);
            return format!("${}\r\n{}\r\n", string_value.len(), string_value).into_bytes();
        }

        b"$-1\r\n".to_vec()
    }
}

#[derive(Debug, PartialEq)]
struct Set {
    key: Vec<u8>,
    value: Vec<u8>,
    px: Option<u64>,
}

impl CommandExt for Set {
    fn parse(parser: &mut ArgParser) -> Result<Self, CommandError> {
        let key = parser.next_bulk_string()?;
        let value = parser.next_bulk_string()?;

        let mut px = None;
        while let Some(peeked_arg) = parser.iter.peek() {
            let RespValue::BulkString(option_bytes) = peeked_arg else {
                return Err(CommandError::InvalidArgument {
                    reason: "syntax error".to_string(),
                });
            };

            match option_bytes.to_ascii_uppercase().as_slice() {
                b"PX" => {
                    parser.iter.next(); // Consume "PX"
                    if px.is_some() {
                        return Err(CommandError::InvalidArgument {
                            reason: "PX option specified multiple times".to_string(),
                        });
                    }
                    let px_val_bytes = parser.next_bulk_string()?;
                    let px_str = str::from_utf8(&px_val_bytes).map_err(|_| {
                        CommandError::InvalidArgument {
                            reason: "PX value must be valid UTF-8".to_string(),
                        }
                    })?;
                    px =
                        Some(
                            px_str
                                .parse::<u64>()
                                .map_err(|_| CommandError::InvalidArgument {
                                    reason: "PX value must be a positive integer".to_string(),
                                })?,
                        );
                }
                _ => {
                    return Err(CommandError::InvalidArgument {
                        reason: "syntax error".to_string(),
                    });
                }
            }
        }

        Ok(Set { key, value, px })
    }

    fn execute(self: Box<Self>, db: &Database) -> Vec<u8> {
        let mut db_lock = db.lock().unwrap();
        let key = String::from_utf8_lossy(&self.key).to_ascii_uppercase();
        let expires_at = self.px.map(|ms| Instant::now() + Duration::from_millis(ms));

        let db_value = DbValue {
            value: self.value.clone(),
            expires_at,
        };
        db_lock.insert(key, db_value);
        b"+OK\r\n".to_vec()
    }
}

pub struct Command(Box<dyn CommandExt + Send>);

impl Command {
    pub fn from_resp(resp: RespValue) -> Result<Self, CommandError> {
        let RespValue::Array(elements) = resp else {
            return Err(CommandError::NotAnArray);
        };
        if elements.is_empty() {
            return Err(CommandError::EmptyCommand);
        }

        let Some(RespValue::BulkString(command_bytes)) = elements.first() else {
            return Err(CommandError::CommandNotBulkString);
        };

        let cmd_name = String::from_utf8_lossy(command_bytes).to_ascii_uppercase();
        let args = &elements[1..];
        let mut parser = ArgParser::new(args);

        let command: Box<dyn CommandExt + Send> = match cmd_name.as_str() {
            "PING" => Box::new(Ping::parse(&mut parser)?),
            "ECHO" => Box::new(Echo::parse(&mut parser)?),
            "GET" => Box::new(Get::parse(&mut parser)?),
            "SET" => Box::new(Set::parse(&mut parser)?),
            _ => return Err(CommandError::UnknownCommand(cmd_name)),
        };

        Ok(Command(command))
    }

    pub fn execute(self, db: &Database) -> Vec<u8> {
        self.0.execute(db)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::RespValue;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    type Database = Arc<Mutex<HashMap<String, DbValue>>>;

    #[test]
    fn test_set_get() {
        let value = b"hello world value";
        let db: Database = Arc::new(Mutex::new(HashMap::new()));
        let resp_value = RespValue::Array(vec![
            RespValue::BulkString(b"SET".to_vec()),
            RespValue::BulkString(b"key".to_vec()),
            RespValue::BulkString(value.to_vec()),
        ]);

        let command = Command::from_resp(resp_value).unwrap();
        let response = command.execute(&db);
        assert_eq!(response, b"+OK\r\n");

        let resp_value = RespValue::Array(vec![
            RespValue::BulkString(b"GET".to_vec()),
            RespValue::BulkString(b"key".to_vec()),
        ]);
        let command = Command::from_resp(resp_value).unwrap();
        let response = command.execute(&db);
        let expected_response =
            format!("${}\r\n{}\r\n", value.len(), String::from_utf8_lossy(value));
        assert_eq!(response, expected_response.into_bytes());
    }

    #[test]
    fn test_ping_command() {
        let db: Database = Arc::new(Mutex::new(HashMap::new()));
        let resp_value = RespValue::Array(vec![RespValue::BulkString(b"PING".to_vec())]);
        let command = Command::from_resp(resp_value).unwrap();
        let response = command.execute(&db);
        assert_eq!(response, b"+PONG\r\n");
    }

    #[test]
    fn test_ping_with_message() {
        let db: Database = Arc::new(Mutex::new(HashMap::new()));
        let msg = b"hello";
        let resp_value = RespValue::Array(vec![
            RespValue::BulkString(b"PING".to_vec()),
            RespValue::BulkString(msg.to_vec()),
        ]);
        let command = Command::from_resp(resp_value).unwrap();
        let response = command.execute(&db);
        let expected = format!("${}\r\n{}\r\n", msg.len(), "hello");
        assert_eq!(response, expected.as_bytes());
    }

    #[test]
    fn test_set_with_px_and_expiration() {
        let db: Database = Arc::new(Mutex::new(HashMap::new()));
        let key = b"key";
        let value = b"value";
        let px_ms = 100u64;
        let set_resp = RespValue::Array(vec![
            RespValue::BulkString(b"SET".to_vec()),
            RespValue::BulkString(key.to_vec()),
            RespValue::BulkString(value.to_vec()),
            RespValue::BulkString(b"PX".to_vec()),
            RespValue::BulkString(px_ms.to_string().into_bytes()),
        ]);

        let command = Command::from_resp(set_resp).unwrap();
        assert_eq!(command.execute(&db), b"+OK\r\n");

        let get_resp = RespValue::Array(vec![
            RespValue::BulkString(b"GET".to_vec()),
            RespValue::BulkString(key.to_vec()),
        ]);
        let get_command = Command::from_resp(get_resp).unwrap();
        let expected_get = format!("${}\r\n{}\r\n", value.len(), String::from_utf8_lossy(value));
        assert_eq!(get_command.execute(&db), expected_get.as_bytes());

        thread::sleep(Duration::from_millis(px_ms + 10));

        let get_resp_after = RespValue::Array(vec![
            RespValue::BulkString(b"GET".to_vec()),
            RespValue::BulkString(key.to_vec()),
        ]);
        let get_command_after = Command::from_resp(get_resp_after).unwrap();
        assert_eq!(get_command_after.execute(&db), b"$-1\r\n");
    }

    #[test]
    fn test_extra_arguments_error() {
        let resp = RespValue::Array(vec![
            RespValue::BulkString(b"GET".to_vec()),
            RespValue::BulkString(b"key".to_vec()),
            RespValue::BulkString(b"extra".to_vec()),
        ]);
        assert!(matches!(
            Command::from_resp(resp),
            Err(CommandError::WrongArgCount)
        ));
    }
}
