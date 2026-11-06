#[cfg(test)]
mod macro_tests {
    use prosa_macros::{FromTvf, IntoTvf, ToTvf, tvf};
    use prosa_utils::msg::{
        simple_string_tvf::SimpleStringTvf,
        tvf::{FromTvf, IntoTvf, ToTvf, Tvf},
    };
    use std::{
        borrow::Cow,
        collections::{BTreeMap, HashMap},
        rc::Rc,
        sync::Arc,
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
            to_tvf = "custom::to_tvf",
            into_tvf = "custom::into_tvf",
            from_tvf = "custom::from_tvf"
        )]
        a: u32,

        b: T,
        c: S,

        #[tvf(skip)]
        d: u32,

        #[tvf(default)]
        e: i32,

        #[tvf(default = "default_float")]
        f: f32,

        #[tvf(skip_to_tvf, default = "default_bool_true")]
        g: bool,

        #[tvf(skip_from_tvf, default = "default_bool_true")]
        h: bool,

        #[tvf(with = "custom")]
        i: u32,
    }

    pub mod custom {
        use prosa_utils::msg::tvf::{Tvf, TvfError};

        pub fn to_tvf<TVF: Tvf>(value: &u32, id: usize, msg: &mut TVF) {
            msg.put_unsigned(id, (*value << 4) as u64);
        }

        pub fn into_tvf<TVF: Tvf>(value: u32, id: usize, msg: &mut TVF) {
            msg.put_unsigned(id, (value << 4) as u64);
        }

        pub fn from_tvf<TVF: Tvf>(msg: &TVF, id: usize) -> Result<u32, TvfError> {
            msg.get_unsigned(id).map(|value| (value as u32) >> 4)
        }
    }

    const fn default_float() -> f32 {
        42.0
    }

    const fn default_bool_true() -> bool {
        true
    }

    #[derive(Debug, PartialEq, Clone, FromTvf, ToTvf, IntoTvf)]
    #[tvf(tag_id = MY_FIELD + 1, tag_type = "unsigned")]
    enum F {
        #[tvf(default, tag = 100)]
        G = 10,

        #[tvf(tag = 120)]
        H = 12,
    }

    #[derive(Debug, PartialEq, Clone, FromTvf, ToTvf, IntoTvf)]
    struct I<'c> {
        vector: Vec<u32>,
        boxed: Box<u32>,
        option: Option<u32>,
        copy_on_write: Cow<'c, u32>,
        hash_map: HashMap<u16, u32>,
        btree_map: BTreeMap<u32, u64>,
        rc: Rc<u32>,
        arc: Arc<String>,
    }

    #[derive(Debug, PartialEq, FromTvf, ToTvf)]
    struct J {
        #[tvf(id = 6, to_tvf = "custom2::to_tvf", from_tvf = "custom2::from_tvf")]
        a: f32,

        #[tvf(id = 7, with = "custom2")]
        b: f32,

        #[tvf(flatten)]
        c: A,
    }

    /// Module without `into_tvf`
    pub mod custom2 {
        use prosa_utils::msg::tvf::{Tvf, TvfError};

        pub fn to_tvf<TVF: Tvf>(value: &f32, id: usize, msg: &mut TVF) {
            msg.put_float(id, (*value * 0.5) as f64);
        }

        pub fn from_tvf<TVF: Tvf>(msg: &TVF, id: usize) -> Result<f32, TvfError> {
            msg.get_float(id).map(|value| (value as f32) * 2.0)
        }
    }

    #[derive(Debug, PartialEq, Clone, FromTvf, ToTvf, IntoTvf)]
    #[tvf(tag_id = MY_FIELD + 1, tag_type = "unsigned")]
    pub enum K {
        #[tvf(tag = 10, with = "custom_variant")]
        Custom { value: u32 },

        #[tvf(tag = 11, skip_to_tvf)]
        SkipTo { value: u32 },

        #[tvf(tag = 12, skip_from_tvf)]
        SkipFrom { value: u32 },

        #[tvf(tag = 13, skip)]
        SkipBoth { value: u32 },

        #[tvf(tag = 14)]
        Regular { value: u32 },

        #[tvf(tag = 15, to_tvf = "custom_variant::to_only")]
        ToOnly { value: u32 },

        #[tvf(tag = 16, into_tvf = "custom_variant::into_only")]
        IntoOnly { value: u32 },
    }

    pub mod custom_variant {
        use super::K;
        use prosa_utils::msg::tvf::{Tvf, TvfError};

        pub fn to_tvf<TVF: Tvf>(value: &K, id: usize, msg: &mut TVF) {
            let K::Custom { value } = value else {
                unreachable!();
            };
            msg.put_unsigned(id + 1, *value as u64);
        }

        pub fn into_tvf<TVF: Tvf>(value: K, id: usize, msg: &mut TVF) {
            let K::Custom { value } = value else {
                unreachable!();
            };
            msg.put_unsigned(id + 1, value as u64);
        }

        pub fn from_tvf<TVF: Tvf>(msg: &TVF, id: usize) -> Result<K, TvfError> {
            msg.get_unsigned(id + 1).map(|value| K::Custom {
                value: value as u32,
            })
        }

        pub fn to_only<TVF: Tvf>(value: &K, id: usize, msg: &mut TVF) {
            let K::ToOnly { value } = value else {
                unreachable!();
            };
            msg.put_unsigned(id + 1, (*value * 10) as u64);
        }

        pub fn into_only<TVF: Tvf>(value: K, id: usize, msg: &mut TVF) {
            let K::IntoOnly { value } = value else {
                unreachable!();
            };
            msg.put_unsigned(id + 1, (value * 10) as u64);
        }
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
            2 => { MY_FIELD => "C" },
            4 => 64,
            5 => 36.5,
            7 => true,
            8 => 1600,
        }];
        let e1 = E::<A, B> {
            a: 12,
            b: A {
                a: 24,
                b: false,
                c: "BYE".to_string(),
            },
            c: B::C,
            d: 0,
            e: 64,
            f: 36.5,
            g: true,
            h: true,
            i: 100,
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
        let fg0 = tvf![SimpleStringTvf { (MY_FIELD + 1) => 100u64 }];
        let fg1 = F::G;
        let mut fg2 = SimpleStringTvf::default();
        fg1.to_tvf(&mut fg2);
        assert_eq!(fg0, fg2);
        let fg3 = F::from_tvf(&fg0).expect("Deserialization from TVF should work");
        assert_eq!(fg1, fg3);

        // H variant
        let fh0 = tvf![SimpleStringTvf { (MY_FIELD + 1) => 120u64 }];
        let fh1 = F::H;
        let mut fh2 = SimpleStringTvf::default();
        fh1.to_tvf(&mut fh2);
        assert_eq!(fh0, fh2);
        let fh3 = F::from_tvf(&fh0).expect("Deserialization from TVF should work");
        assert_eq!(fh1, fh3);
    }

    #[test]
    fn test_derive_std_types() {
        let i0 = tvf![SimpleStringTvf {
            0 => [ 10, 20, 30, 40 ],
            1 => 64,
            // field 2 is skipped
            3 => 128,
            4 => { 2 => 40, 4 => 80 },
            5 => { 3 => 50, 5 => 90 },
            6 => 250,
            7 => "shared",
        }];
        let i1 = I {
            vector: vec![10, 20, 30, 40],
            boxed: Box::new(64),
            option: None,
            copy_on_write: Cow::Owned(128),
            hash_map: {
                let mut map = HashMap::new();
                map.insert(2, 40);
                map.insert(4, 80);
                map
            },
            btree_map: {
                let mut map = BTreeMap::new();
                map.insert(3, 50);
                map.insert(5, 90);
                map
            },
            rc: Rc::new(250),
            arc: Arc::new(String::from("shared")),
        };

        // serialize to TVF
        let mut i2 = SimpleStringTvf::default();
        i1.to_tvf(&mut i2);
        assert_eq!(i0, i2);

        let mut i2 = SimpleStringTvf::default();
        i1.clone().into_tvf(&mut i2);
        assert_eq!(i0, i2);

        // deserialize from TVF
        let i3 = I::from_tvf(&i0).expect("Deserialization from TVF should work");
        assert_eq!(i1, i3);

        // second test

        let i0 = tvf![SimpleStringTvf {
            0 => [],
            1 => 32,
            2 => 33,
            3 => 34,
            4 => {},
            5 => {},
            6 => 35,
            7 => "arc",
        }];

        let int = 34;
        let i1 = I {
            vector: Vec::new(),
            boxed: Box::new(32),
            option: Some(33),
            copy_on_write: Cow::Borrowed(&int),
            hash_map: HashMap::new(),
            btree_map: BTreeMap::new(),
            rc: Rc::new(35),
            arc: Arc::new(String::from("arc")),
        };

        // serialize to TVF
        let mut i2 = SimpleStringTvf::default();
        i1.to_tvf(&mut i2);
        assert_eq!(i0, i2);

        let mut i2 = SimpleStringTvf::default();
        i1.clone().into_tvf(&mut i2);
        assert_eq!(i0, i2);

        // deserialize from TVF
        let i3 = I::from_tvf(&i0).expect("Deserialization from TVF should work");
        assert_eq!(i1, i3);
    }

    #[test]
    fn test_derive_custom_field_paths() {
        let expected = tvf![SimpleStringTvf {
            0 => 12u64,
            10 => 1u8,
            MY_FIELD => "HELLO",
            6 => 1.5,
            7 => 2.0,
        }];
        let value = J {
            a: 3.0,
            b: 4.0,
            c: A {
                a: 12,
                b: true,
                c: String::from("HELLO"),
            },
        };

        let mut encoded = SimpleStringTvf::default();
        value.to_tvf(&mut encoded);
        assert_eq!(expected, encoded);

        let decoded = J::from_tvf(&expected).expect("Custom field paths should deserialize");
        assert_eq!(value, decoded);
    }

    #[test]
    fn test_derive_enum_variant_overrides() {
        let expected_custom = tvf![SimpleStringTvf {
            (MY_FIELD + 1) => 10u64,
            (MY_FIELD + 2) => 42u64,
        }];
        let custom = K::Custom { value: 42 };

        let mut encoded = SimpleStringTvf::default();
        custom.to_tvf(&mut encoded);
        assert_eq!(expected_custom, encoded);

        let mut encoded = SimpleStringTvf::default();
        custom.clone().into_tvf(&mut encoded);
        assert_eq!(expected_custom, encoded);
        assert_eq!(custom, K::from_tvf(&expected_custom).unwrap());

        let skipped_to = K::SkipTo { value: 7 };
        let mut encoded = SimpleStringTvf::default();
        skipped_to.to_tvf(&mut encoded);
        assert!(encoded.is_empty());

        let mut encoded = SimpleStringTvf::default();
        skipped_to.into_tvf(&mut encoded);
        assert!(encoded.is_empty());

        let skipped_from = K::SkipFrom { value: 8 };
        let mut encoded = SimpleStringTvf::default();
        skipped_from.to_tvf(&mut encoded);
        assert!(K::from_tvf(&encoded).is_err());

        let skipped_both = K::SkipBoth { value: 9 };
        let mut encoded = SimpleStringTvf::default();
        skipped_both.to_tvf(&mut encoded);
        assert!(encoded.is_empty());

        let skipped_tag = tvf![SimpleStringTvf { (MY_FIELD + 1) => 13u64 }];
        assert!(K::from_tvf(&skipped_tag).is_err());

        let regular = K::Regular { value: 12 };
        let mut encoded = SimpleStringTvf::default();
        regular.to_tvf(&mut encoded);
        assert_eq!(regular, K::from_tvf(&encoded).unwrap());

        let to_only = K::ToOnly { value: 5 };
        let expected_to_only = tvf![SimpleStringTvf {
            (MY_FIELD + 1) => 15u64,
            (MY_FIELD + 2) => 50u64,
        }];
        let mut encoded = SimpleStringTvf::default();
        to_only.to_tvf(&mut encoded);
        assert_eq!(expected_to_only, encoded);

        let expected_to_only_into = tvf![SimpleStringTvf {
            (MY_FIELD + 1) => 15u64,
            0 => 5u32,
        }];
        let mut encoded = SimpleStringTvf::default();
        to_only.into_tvf(&mut encoded);
        assert_eq!(expected_to_only_into, encoded);

        let into_only = K::IntoOnly { value: 6 };
        let expected_into_only_to = tvf![SimpleStringTvf {
            (MY_FIELD + 1) => 16u64,
            0 => 6u32,
        }];
        let mut encoded = SimpleStringTvf::default();
        into_only.to_tvf(&mut encoded);
        assert_eq!(expected_into_only_to, encoded);

        let expected_into_only = tvf![SimpleStringTvf {
            (MY_FIELD + 1) => 16u64,
            (MY_FIELD + 2) => 60u64,
        }];
        let mut encoded = SimpleStringTvf::default();
        into_only.into_tvf(&mut encoded);
        assert_eq!(expected_into_only, encoded);
    }
}
