use prosa_macros::{FromTvf, ToTvf};

/// Define fields' identifier as constants
const MY_FIELD: usize = 100;

#[derive(Debug, FromTvf, ToTvf)]
struct A {
    a: u32,

    #[tvf(id = 10)]
    b: bool,

    #[tvf(id = MY_FIELD)]
    c: String,
}

#[derive(Debug, FromTvf, ToTvf)]
#[tvf(tag_id = 100, tag_type = "string")]
enum B {
    C,
    D { a: u32, b: f32 },
}

#[cfg(test)]
mod macro_tests {

    #[test]
    fn test_to_tvf_macro() {}
}
