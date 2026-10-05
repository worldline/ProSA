#[cfg(test)]
mod macro_tests {
    use prosa_macros::{FromTvf, ToTvf, tvf};
    use prosa_utils::msg::{
        simple_string_tvf::SimpleStringTvf,
        tvf::{FromField, FromTvf, ToField, ToTvf, Tvf},
    };

    /// Define fields' identifier as constants
    const MY_FIELD: usize = 100;

    #[derive(Debug, PartialEq, FromTvf, ToTvf)]
    struct A {
        a: u32,

        #[tvf(id = 10)]
        b: bool,

        #[tvf(id = MY_FIELD)]
        c: String,
    }

    // TODO: to be implemented by derive macro
    impl<__TVF: Tvf + Clone> FromField<__TVF> for A {
        fn from_field(msg: &__TVF, id: usize) -> Result<Self, prosa_utils::msg::tvf::TvfError> {
            let sub = msg.get_buffer(id)?;
            A::from_tvf(sub.as_ref())
        }
    }

    // TODO: to be implemented by derive macro
    impl<__TVF: Tvf + Default> ToField<__TVF> for A {
        fn to_field(&self, id: usize, msg: &mut __TVF) {
            let mut sub = __TVF::default();
            self.to_tvf(&mut sub);
            msg.put_buffer(id, sub);
        }
    }

    #[derive(Debug, PartialEq, FromTvf, ToTvf)]
    #[tvf(tag_id = MY_FIELD)]
    enum B {
        C,
        D { a: u32, b: f32 },
    }

    #[derive(Debug, PartialEq, FromTvf, ToTvf)]
    struct E<T> {
        a: u32,
        b: T,
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
        let a3 = A::from_tvf(&a0).unwrap();
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
        let bc3 = B::from_tvf(&bc0).unwrap();
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
        let bd3 = B::from_tvf(&bd0).unwrap();
        assert_eq!(bd1, bd3);
    }

    #[test]
    fn test_derive_generic() {
        let e0 = tvf![SimpleStringTvf {
            0 => 12u64,
            1 => {
                0   => 24u64,
                10  => 0u8,
                100 => "BYE",
            },
        }];
        let e1 = E::<A> {
            a: 12,
            b: A {
                a: 24,
                b: false,
                c: "BYE".to_string(),
            },
        };

        // serialize to TVF
        let mut e2 = SimpleStringTvf::default();
        e1.to_tvf(&mut e2);
        assert_eq!(e0, e2);

        // deserialize from TVF
        let e3 = E::<A>::from_tvf(&e0).unwrap();
        assert_eq!(e1, e3);
    }
}
