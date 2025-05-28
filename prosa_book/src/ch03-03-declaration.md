# Processor declaration

In the previous chapter, you learned how to declare your [Adaptor](ch02-02-declaration.md).
Now it's time to declare your processor.

As with the Adaptor, you need to declare your processor using [cargo metadata](https://doc.rust-lang.org/cargo/reference/manifest.html#the-metadata-table).
In your _Cargo.toml_, you should include a section like this:
```toml
[package.metadata.prosa.<processor_name>]
proc = "<your crate name>::<path to your processor>"
settings = "<your crate name>::<path to your processor's settings>"
adaptor = []
```

Of course, in this section you can also list adaptors. You may have generic adaptors that cover most cases.

For an example, see [ProSA - Cargo.toml](https://github.com/worldline/ProSA/blob/main/prosa/Cargo.toml#L19).

If you declare this metadata correctly, you will be able to see your processor, settings, and adaptors when using [cargo-prosa](http://localhost:3000/ch01-01-cargo-prosa.html#use).
