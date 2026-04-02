#[cfg(test)]
mod macro_tests {
    use bytes::Bytes;
    use chrono::NaiveDate;
    use prosa_macros::tvf;
    use prosa_utils::msg::simple_string_tvf::SimpleStringTvf;
    use prosa_utils::msg::tvf::Tvf;

    #[test]
    fn test_tvf_macro() {
        let buffer = tvf!(SimpleStringTvf {
            1 => 2,
            3 => 4usize,
            5 => [
                1 as Unsigned,
                2 as Float,
                3,
                "four",
                {
                    1 => "object",
                    2 => 0x00010203_04050607_08090A0B_0C0D0E0F_10111213_14151617_18191A1B_1C1D1E1F as Bytes
                }
            ],
            6 => "1995-01-10" as Date,
            200 => "2023-06-05 15:02:00.000" as DateTime,
        });

        assert_eq!(5, buffer.len());
        assert_eq!(Ok(2), buffer.get_unsigned(1));
        assert_eq!(Ok(4), buffer.get_signed(3));

        let subbuffer = buffer.get_buffer(5).expect("TVF should have a sub buffer");
        assert_eq!(Ok(1u64), subbuffer.get_unsigned(1));
        assert_eq!(Ok(2.0f64), subbuffer.get_float(2));
        assert_eq!(Ok(3i64), subbuffer.get_signed(3));
        assert_eq!(
            Ok("four"),
            subbuffer.get_string(4).map(|s| s.to_string()).as_deref()
        );

        let sub = subbuffer
            .get_buffer(5)
            .expect("TVF should have a sub buffer");
        assert_eq!(
            Ok("object"),
            sub.get_string(1).map(|s| s.to_string()).as_deref()
        );
        assert_eq!(
            Ok(Bytes::from_static(&[
                0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D,
                0x0E, 0x0F, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x1B,
                0x1C, 0x1D, 0x1E, 0x1F
            ])),
            sub.get_bytes(2).as_deref().cloned()
        );
        assert_eq!(
            Ok(NaiveDate::from_ymd_opt(1995, 1, 10).expect("NaiveDate should be build")),
            buffer.get_date(6)
        );
        assert_eq!(
            Ok(NaiveDate::from_ymd_opt(2023, 6, 5)
                .expect("NaiveDate should be build")
                .and_hms_opt(15, 2, 0)
                .expect("NaiveDateTime should be build")),
            buffer.get_datetime(200)
        );
    }
}
