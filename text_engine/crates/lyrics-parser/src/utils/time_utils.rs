pub fn is_digits_only(value: &str) -> bool {
    value.chars().all(|ch| ch.is_ascii_digit())
}

fn digits_to_int_or0(value: &str, start: usize, end: usize) -> i32 {
    if start >= end {
        return 0;
    }
    let bytes = value.as_bytes();
    let mut result = 0i32;
    for index in start..end.min(bytes.len()) {
        let byte = bytes[index];
        if !byte.is_ascii_digit() {
            return 0;
        }
        result = result * 10 + (byte - b'0') as i32;
    }
    result
}

fn parse_seconds_and_millis(value: &str, start: usize, end: usize) -> i32 {
    let dot = value[start..end].find('.').map(|index| start + index);
    let Some(dot) = dot else {
        return digits_to_int_or0(value, start, end) * 1000;
    };

    let seconds = digits_to_int_or0(value, start, dot) * 1000;
    let millis_start = dot + 1;
    let millis_len = end.saturating_sub(millis_start);
    if millis_len == 0 {
        return seconds;
    }

    let take = millis_len.min(3);
    let mut millis = digits_to_int_or0(value, millis_start, millis_start + take);
    if millis_len == 1 {
        millis *= 100;
    } else if millis_len == 2 {
        millis *= 10;
    }
    seconds + millis
}

pub fn parse_as_time(value: &str) -> i32 {
    if value.is_empty() {
        return 0;
    }

    let Some(first_colon) = value.find(':') else {
        return parse_seconds_and_millis(value, 0, value.len());
    };
    let last_colon = value.rfind(':').unwrap_or(first_colon);
    if first_colon == last_colon {
        digits_to_int_or0(value, 0, first_colon) * 60_000
            + parse_seconds_and_millis(value, first_colon + 1, value.len())
    } else {
        digits_to_int_or0(value, 0, first_colon) * 3_600_000
            + digits_to_int_or0(value, first_colon + 1, last_colon) * 60_000
            + parse_seconds_and_millis(value, last_colon + 1, value.len())
    }
}

pub fn to_time_formatted_string(total_millis: i32) -> String {
    if total_millis < 0 {
        return "00:00.000".to_string();
    }

    let total_seconds = total_millis / 1000;
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;
    let millis = total_millis % 1000;
    format!("{minutes:02}:{seconds:02}.{millis:03}")
}
