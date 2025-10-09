#[derive(Debug, PartialEq, Clone)]
pub enum RespValue {
    SimpleString(String),
    Integer(i64),
    BulkString(Vec<u8>),
    Array(Vec<RespValue>),
    Null,
}

#[derive(Debug, PartialEq)]
pub enum ParserError {
    Incomplete,
    InvalidFormat(String),
}

type ParseResult = Result<(RespValue, usize), ParserError>;

impl RespValue {
    pub fn parse(buffer: &[u8]) -> ParseResult {
        if buffer.is_empty() {
            return Err(ParserError::Incomplete);
        }
        match buffer[0] {
            b':' => Self::parse_integer(buffer),
            b'+' => Self::parse_simple_string(buffer),
            b'$' => Self::parse_bulk_string(buffer),
            b'*' => Self::parse_array(buffer),
            _ => Err(ParserError::InvalidFormat("Unknown prefix".to_string())),
        }
    }

    fn parse_line(buffer: &[u8]) -> Result<(&[u8], usize), ParserError> {
        if let Some(pos) = buffer.windows(2).position(|window| window == b"\r\n") {
            let line = &buffer[1..pos];
            let consumed = pos + 2;
            Ok((line, consumed))
        } else {
            Err(ParserError::Incomplete)
        }
    }

    fn parse_integer(buffer: &[u8]) -> ParseResult {
        let (line, consumed) = Self::parse_line(buffer)?;
        let s = std::str::from_utf8(line).map_err(|e| ParserError::InvalidFormat(e.to_string()))?;
        let val = s
            .parse::<i64>()
            .map_err(|e| ParserError::InvalidFormat(e.to_string()))?;
        Ok((RespValue::Integer(val), consumed))
    }

    fn parse_simple_string(buffer: &[u8]) -> ParseResult {
        let (line, consumed) = Self::parse_line(buffer)?;
        let s = String::from_utf8(line.to_vec())
            .map_err(|e| ParserError::InvalidFormat(e.to_string()))?;
        Ok((RespValue::SimpleString(s), consumed))
    }

    fn parse_bulk_string(buffer: &[u8]) -> ParseResult {
        let (len_bytes, header_consumed) = Self::parse_line(buffer)?;
        let len_str = std::str::from_utf8(len_bytes)
            .map_err(|e| ParserError::InvalidFormat(e.to_string()))?;

        let len = len_str
            .parse::<isize>()
            .map_err(|e| ParserError::InvalidFormat(e.to_string()))?;

        if len == -1 {
            return Ok((RespValue::Null, header_consumed));
        }

        let len = len as usize;
        let total_len = header_consumed + len + 2; // +CRLF
        if buffer.len() < total_len {
            return Err(ParserError::Incomplete);
        }

        if &buffer[header_consumed + len..total_len] != b"\r\n" {
            return Err(ParserError::InvalidFormat(
                "Missing CRLF after bulk string data".to_string(),
            ));
        }

        let data = buffer[header_consumed..header_consumed + len].to_vec();
        Ok((RespValue::BulkString(data), total_len))
    }

    fn parse_array(buffer: &[u8]) -> ParseResult {
        let (len_bytes, mut consumed) = Self::parse_line(buffer)?;
        let len_str = std::str::from_utf8(len_bytes)
            .map_err(|e| ParserError::InvalidFormat(e.to_string()))?;

        let len = len_str
            .parse::<isize>()
            .map_err(|e| ParserError::InvalidFormat(e.to_string()))?;

        if len == -1 {
            return Ok((RespValue::Null, consumed));
        }

        let len = len as usize;
        let mut elements = Vec::with_capacity(len);

        for _ in 0..len {
            let (element, element_consumed) = Self::parse(&buffer[consumed..])?;
            elements.push(element);
            consumed += element_consumed;
        }

        Ok((RespValue::Array(elements), consumed))
    }
}

#[cfg(test)]
mod tests {
    use crate::parser::RespValue;

    #[test]
    fn test_integer_parsing() {
        let buffer = b":123\r\n";
        println!("--- Testing Integer ---");
        let (value, consumed) = RespValue::parse(buffer).unwrap();
        println!("Parsed value: {:?}", value);

        assert_eq!(value, RespValue::Integer(123));
        assert_eq!(consumed, buffer.len());
    }

    #[test]
    fn test_simple_string_parsing() {
        let buffer = b"+OK\r\n";
        println!("--- Testing Simple String ---");
        let (value, consumed) = RespValue::parse(buffer).unwrap();
        println!("Parsed value: {:?}", value);

        assert_eq!(value, RespValue::SimpleString("OK".to_string()));
        assert_eq!(consumed, buffer.len());
    }

    #[test]
    fn test_bulk_string_ping() {
        let buffer = b"$4\r\nPING\r\n";
        let (value, consumed) = RespValue::parse(buffer).unwrap();
        println!("--- Testing Bulk String 'PING' ---");
        println!("Parsed value: {:?}", value);

        assert_eq!(value, RespValue::BulkString(b"PING".to_vec()));
        assert_eq!(consumed, buffer.len());
    }

    #[test]
    fn test_array_one_element_ping() {
        let buffer = b"*1\r\n$4\r\nPING\r\n";
        let (value, consumed) = RespValue::parse(buffer).unwrap();
        println!("--- Testing Array with one 'PING' element ---");
        println!("Parsed value: {:?}", value);

        let expected_array = RespValue::Array(vec![RespValue::BulkString(b"PING".to_vec())]);
        assert_eq!(value, expected_array);
        assert_eq!(consumed, buffer.len());
    }
}
