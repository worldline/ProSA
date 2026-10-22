# TVF

**T**ag **V**alue **F**ield is the internal message interface used by ProSA.

It's not a full-fledged format but a [Rust trait](https://docs.rs/prosa-utils/latest/prosa_utils/msg/tvf/trait.Tvf.html) that defines what a message format should support.

Currently, only the [SimpleStringTvf](https://docs.rs/prosa-utils/latest/prosa_utils/msg/simple_string_tvf/struct.SimpleStringTvf.html) implementation exists.
However, in the future, others could implement the TVF trait, such as [ProtoBuf](https://docs.rs/protobuf/latest/protobuf/), and more.

## Usage

In ProSA, [`Tvf`](https://docs.rs/prosa-utils/latest/prosa_utils/msg/tvf/trait.Tvf.html) is used as a generic message type that can support various serialization strategies.
The trait allows you to:
- Add fields using `put_*` methods
- Retrieve fields using `get_*` methods
- Access information from the container
- ...

Most of the time, when using a component that use TVF, you'll see a generic declaration like:
```rust,ignore
struct StructObject<M>
where
    M: 'static
        + std::marker::Send
        + std::marker::Sync
        + std::marker::Sized
        + std::clone::Clone
        + std::fmt::Debug
        + prosa_utils::msg::tvf::Tvf
        + std::default::Default,
{
    fn create_tvf() -> M {
        let buffer = M::default();
        buffer.put_string(1, "value");
        println!("TVF contains: {buffer:?}");
        buffer
    }
}
```

> To create a TVF, the `Default` trait must be implemented.

> Good to have are `Clone` and `Debug` for your TVF. When TVFs are used for messaging, `Send` and `Sync` are essential to safely move them across threads.

## Constructing TVF Messages with the `tvf!` Macro

Instead of manually calling `put_*` methods one by one, you can use the `tvf!` procedural macro to construct TVF messages with a concise syntax.

### Basic usage (map)

Create a TVF with key-value pairs using `=>`:

```rust,ignore
use prosa_macros::tvf;
use prosa_utils::msg::simple_string_tvf::SimpleStringTvf;

let message = tvf!(SimpleStringTvf {
    1 => "hello",
    2 => 42,
    3 => 3.14,
    4 => false,
    5 => true,
});
```

Integer values are inferred as signed (`i64`) by default. Boolean values are stored as bytes:
`false` becomes `0u8` and `true` becomes `1u8`. Use type suffixes or `as` casts to specify other
types:

```rust,ignore
let message = tvf!(SimpleStringTvf {
    1 => 2,              // signed (i64)
    2 => 4usize,         // also signed
    3 => 1 as Unsigned,  // unsigned (u64)
    4 => 2.5 as Float,   // float (f64)
});
```

### Type annotations

Use `as Type` to explicitly set the field type:

| Annotation    | Rust type       | Example                                 |
|---------------|-----------------|-----------------------------------------|
| `as Unsigned` | `u64`           | `1 as Unsigned`                         |
| `as Signed`   | `i64`           | `-5 as Signed`                          |
| `as Float`    | `f64`           | `3.14 as Float`                         |
| `as Byte`     | `u8`            | `22 as Byte`                            |
| `as String`   | `String`         | `b"UTF-8 bytes" as String`              |
| `as Bytes`    | `Bytes`         | `0x01020304 as Bytes`                   |
| `as Date`     | `NaiveDate`     | `"1995-01-10" as Date`                  |
| `as DateTime` | `NaiveDateTime` | `"2023-06-05 15:02:00.000" as DateTime` |

Converting a byte-string literal with `as String` happens at compile time and fails compilation if
the bytes are not valid UTF-8.

### Lists

Use `[ ]` to create sequential fields indexed starting from 1:

```rust,ignore
let list = tvf!(SimpleStringTvf [
    "first element",   // index 1
    "second element",  // index 2
    42,                // index 3
]);
```

### Nested buffers

Nest TVF structures inside other TVF messages:

```rust,ignore
let message = tvf!(SimpleStringTvf {
    1 => 2,
    5 => [
        1 as Unsigned,
        2 as Float,
        3,
        "four",
        5 as Byte,
        {
            1 => "nested object",
            2 => 0x00010203 as Bytes,
        }
    ],
    6 => "1995-01-10" as Date,
    7 => false,
    8 => b"UTF-8 bytes" as String,
    200 => "2023-06-05 15:02:00.000" as DateTime,
});
```

Lists can contain nested maps (`{ }`), and maps can contain nested lists (`[ ]`), allowing arbitrary depth.


## Constructing TVF Messages with the derive macros

You can also define TVF messages by applying derive macros on Rust's data types.
The derives in question are:
- `ToTvf` to convert a data type into a TVF message
- `IntoTvf` to consume and convert a data type into a TVF message
- `FromTvf` to reconstruct the data type from a TVF message

Each of the TVF derives has its corresponding field trait that are also implemented allowing to construct hierarchical TVF messages through Rust's data composition:
- `ToField` 
- `IntoField` 
- `FromField` 


### Basic usage

You can apply the derive macros to either structures or enumerations (unions are not supported).
By default, fields are assigned identifier in the order they are defined (1, 2, 3, ...).
Those identifiers can be overridden using the `id` attribute.

```rust,ignore
const MY_FIELD: usize = 100;

#[derive(Debug, PartialEq, Clone, FromTvf, ToTvf, IntoTvf)]
struct A {
    a: u32,
    
    #[tvf(id = 10)]
    b: bool,
    
    #[tvf(id = MY_FIELD)]
    c: String,
}
```

When applied to an enumeration, ones have to specify the field used to encode the variant discriminant using the `tag_id` attribute.
The `tag_type` attribute on the other hand is optional with the following possible values:
- `string` (the default)
- `byte`
- `signed`
- `unsigned`

```rust,ignore
#[derive(Debug, PartialEq, Clone, FromTvf, ToTvf, IntoTvf)]
#[tvf(tag_id = 123, tag_type = "byte")]
enum B {
    C,
    D { a: u32, b: f32 },
}
```

It is also possible to override the conversion method for a specific field using the following attributes with the path to the function to use:
- `to_tvf`
- `into_tvf`
- `from_tvf`

```rust,ignore
#[derive(Debug, PartialEq, FromTvf, ToTvf, IntoTvf)]
struct E {
    #[tvf(to_tvf = "custom_to", into_tvf = "custom_into", from_tvf = "custom_from")]
    a: u32,
    b: f32,
}

fn custom_to<TVF: Tvf>(value: &u32, id: usize, msg: &mut TVF) {
    msg.put_unsigned(id, (*value << 4) as u64);
}

fn custom_into<TVF: Tvf>(value: u32, id: usize, msg: &mut TVF) {
    msg.put_unsigned(id, (value << 4) as u64);
}

fn custom_from<TVF: Tvf>(msg: &TVF, id: usize) -> Result<u32, TvfError> {
    msg.get_unsigned(id).map(|value| (value as u32) >> 4)
}
```



## Implement your own TVF

If you have your own internal format, you can implement the TVF trait on your own and expose your TVF struct:
```rust,ignore
impl Tvf for MyOwnTvf {
    // All trait method must be implement here
}
```

Make sure to also implement:
- `Default`: to create an empty or initial TVF
- `Send`/`Sync`: to safely transfer across threads
- `Clone`: if duplication of buffers is needed
- `Debug`: To enable easy debugging and inspection

### Use integer hashing for TVF fields

TVF field identifiers are integers. When designing a TVF buffer backed by a hash
table, use [`IntHashMap`](https://docs.rs/prosa-utils/latest/prosa_utils/hash/type.IntHashMap.html)
instead of the standard `HashMap`. It uses ProSA's integer hasher and avoids the
overhead of general-purpose hashing for field lookups.

```rust,ignore
use prosa_utils::hash::IntHashMap;

#[derive(Default)]
struct MyOwnTvf {
    fields: IntHashMap<usize, String>,
}
```

`IntHashMap` supports only integer key types, including `usize`, which matches the
field identifiers used by the `Tvf` trait. For an integer set, use
[`IntHashSet`](https://docs.rs/prosa-utils/latest/prosa_utils/hash/type.IntHashSet.html).

## Declare your custom TVF

When you implement your own TVF, you need to expose it in your Cargo.toml metadata as discussed in the previous chapter.

To do this, add the following to your _Cargo.toml_ file:
```toml
[package.metadata.prosa]
tvf = ["tvf::MyOwnTvf"]
```

Be sure to specify the entire path of your implementation, `tvf::MyOwnTvf`, in this case, if you place it in _src/tvf.rs_.

## Handling sensitive data

At Worldline, since we process payments, messages may contain sensitive data.
This data must not be printed or extracted from the application to ensure security.

To address this, ProSA provides the [`TvfFilter`](https://docs.rs/prosa-utils/latest/prosa_utils/msg/tvf/trait.TvfFilter.html) trait, which allows filtering and masking sensitive fields.

Depending on your message, sensitive field may vary.
Since `TvfFilter` is a trait, you can implement your own filter tailored to your message format.
