# TVF

**T**ag **V**alue **F**ield is the internal message interface used by ProSA.

It's not a full-fledged format but a [Rust trait](https://docs.rs/prosa-utils/latest/prosa_utils/msg/tvf/trait.Tvf.html) that defines what a message format should support.

Currently, only the [SimpleStringTvf](https://docs.rs/prosa-utils/latest/prosa_utils/msg/simple_string_tvf/struct.SimpleStringTvf.html) implementation exists.
However, in the future, others could implement the TVF trait, such as [ProtoBuf](https://docs.rs/protobuf/latest/protobuf/), and more.

## Usage

In ProSA, [`Tvf`](https://docs.rs/prosa-utils/latest/prosa_utils/msg/tvf/trait.Tvf.html) is used as a generic to support multiple implementation.
The trait allows you to:
- Add fields using `put_*` methods
- Retrieve fields using `get_*` methods
- Access information from the container
- ...

Most of the time, when using a component that use TVF, you'll see a generic declaration like:
```rust,noplayground
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

## Implement your own TVF

If you have your own internal format (as we do at Worldline), you can implement the TVF trait on your own and expose your TVF struct:
```rust,noplayground
impl Tvf for MyOwnTvf {
    // All trait method must be implement here
}
```

Make sure to also implement:
- `Default`: to create an empty or initial TVF
- `Send`/`Sync`: to safely transfer across threads
- `Clone`: if duplication of buffers is needed
- `Debug`: To enable easy debugging and inspection

## Declare your custom TVF

When you implement your own TVF, you need to expose it in your Cargo.toml metadata as discussed in the previous chapter.

To do this, add the following to your _Cargo.toml_ file:
```toml
[package.metadata.prosa]
tvf = ["tvf::MyOwnTvf"]
```

Be sure to specify the entire path of your implementation, `tvf::MyOwnTvf`, in this case, if you place it in _src/tvf.rs_.

## Handling sensitive data

At Worldline, since we process payments, buffers may contain sensitive data.
This data must not be printed or extracted from the application to ensure security.

To address this, ProSA provides the [`TvfFilter`](https://docs.rs/prosa-utils/latest/prosa_utils/msg/tvf/trait.TvfFilter.html) trait, which allows filtering and masking sensitive data.

Depending on your message, sensitive field may vary.
Since `TvfFilter` is a trait, you can implement your own filter tailored to your message format.
