#[cfg(test)]
mod macro_tests {
    use prosa_macros::{FromTvf, IntoTvf, ToTvf, tvf};
    use prosa_utils::msg::{
        simple_string_tvf::SimpleStringTvf,
        tvf::{FromTvf, IntoTvf, ToTvf, Tvf, TvfError},
    };

    /// Define fields' identifier as constants
    const MY_FIELD: usize = 100;

    #[derive(Debug, PartialEq, Clone, FromTvf, ToTvf, IntoTvf)]
    struct A {
        a: u32,

        #[tvf(id = 10)]
        b: bool,

        #[tvf(id = MY_FIELD)]
        c: String,
    }

    #[derive(Debug, PartialEq, Clone, FromTvf, ToTvf, IntoTvf)]
    #[tvf(tag_id = MY_FIELD, tag_type = "string")]
    enum B {
        C,
        D { a: u32, b: f32 },
    }

    #[derive(Debug, PartialEq, FromTvf, ToTvf, IntoTvf)]
    struct E<T: Clone, S>
    where
        S: Clone,
    {
        #[tvf(
            to_tvf = "custom_to_tvf",
            into_tvf = "custom_into_tvf",
            from_tvf = "custom_from_tvf"
        )]
        a: u32,
        b: T,
        c: S,
    }

    #[derive(Debug, PartialEq, Clone, FromTvf, ToTvf, IntoTvf)]
    #[tvf(tag_id = MY_FIELD + 1, tag_type = "unsigned")]
    enum F {
        G = 10,
        H = 12,
    }

    fn custom_to_tvf<TVF: Tvf>(value: &u32, id: usize, msg: &mut TVF) {
        msg.put_unsigned(id, (*value << 4) as u64);
    }

    fn custom_into_tvf<TVF: Tvf>(value: u32, id: usize, msg: &mut TVF) {
        msg.put_unsigned(id, (value << 4) as u64);
    }

    fn custom_from_tvf<TVF: Tvf>(msg: &TVF, id: usize) -> Result<u32, TvfError> {
        msg.get_unsigned(id).map(|value| (value as u32) >> 4)
    }

    #[test]
    fn test_derive_struct() {
        let a0 = tvf![SimpleStringTvf {
            0   => 12u64,
            10  => 1u8,
            100 => "HELLO",
        }];
        let a1 = A {
            a: 12,
            b: true,
            c: "HELLO".to_string(),
        };

        // serialize to TVF
        let mut a2 = SimpleStringTvf::default();
        a1.to_tvf(&mut a2);
        assert_eq!(a0, a2);

        // deserialize from TVF
        let a3 = A::from_tvf(&a0).expect("Deserialization from TVF should work");
        assert_eq!(a1, a3);
    }

    #[test]
    fn test_derive_enum() {
        // C variant
        let bc0 = tvf![SimpleStringTvf { MY_FIELD => "C" }];
        let bc1 = B::C;
        let mut bc2 = SimpleStringTvf::default();
        bc1.to_tvf(&mut bc2);
        assert_eq!(bc0, bc2);
        let bc3 = B::from_tvf(&bc0).expect("Deserialization from TVF should work");
        assert_eq!(bc1, bc3);

        // D variant
        let bd0 = tvf![SimpleStringTvf {
            MY_FIELD => "D",
            0 => 123,
            1 => 0.125,
        }];
        let bd1 = B::D { a: 123, b: 0.125 };
        let mut bd2 = SimpleStringTvf::default();
        bd1.to_tvf(&mut bd2);
        assert_eq!(bd0, bd2);
        let bd3 = B::from_tvf(&bd0).expect("Deserialization from TVF should work");
        assert_eq!(bd1, bd3);
    }

    #[test]
    fn test_derive_generic() {
        let e0 = tvf![SimpleStringTvf {
            0 => 192u64,
            1 => {
                0   => 24u64,
                10  => 0u8,
                100 => "BYE",
            },
            2 => { MY_FIELD => "C" }
        }];
        let e1 = E::<A, B> {
            a: 12,
            b: A {
                a: 24,
                b: false,
                c: "BYE".to_string(),
            },
            c: B::C,
        };

        // serialize to TVF
        let mut e2 = SimpleStringTvf::default();
        e1.to_tvf(&mut e2);
        assert_eq!(e0, e2);

        // deserialize from TVF
        let e3 = E::<A, B>::from_tvf(&e0).expect("Deserialization from TVF should work");
        assert_eq!(e1, e3);
    }

    #[test]
    fn test_derive_enum2() {
        // G variant
        let fg0 = tvf![SimpleStringTvf { (MY_FIELD + 1) => 10 as Unsigned }];
        let fg1 = F::G;
        let mut fg2 = SimpleStringTvf::default();
        fg1.to_tvf(&mut fg2);
        assert_eq!(fg0, fg2);
        let fg3 = F::from_tvf(&fg0).expect("Deserialization from TVF should work");
        assert_eq!(fg1, fg3);

        // H variant
        let fh0 = tvf![SimpleStringTvf { (MY_FIELD + 1) => 12 as Unsigned }];
        let fh1 = F::H;
        let mut fh2 = SimpleStringTvf::default();
        fh1.to_tvf(&mut fh2);
        assert_eq!(fh0, fh2);
        let fh3 = F::from_tvf(&fh0).expect("Deserialization from TVF should work");
        assert_eq!(fh1, fh3);
    }
}
