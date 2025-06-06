# Adaptor declaration

As you saw with [cargo-prosa](ch01-01-cargo-prosa.md), available adaptor can be listed using `cargo prosa list`.
This allows you to easily add your adaptor to the _ProSA.toml_ configuration file.

To build this list, cargo-prosa leverages [cargo metadata](https://doc.rust-lang.org/cargo/reference/manifest.html#the-metadata-table).
Thanks to this, it can retrieve metadata from your dependencies and show the list of adaptors you have defined.

To declare your own adaptor, add the following metadata to your _Cargo.toml_:
```toml
[package.metadata.prosa.<processor_name>]
adaptor = ["<your crate name>::<path to your adaptor>"]
```

An adaptor is always related to a processor. That's why you need to declare your adaptor under the relevant processor name.

The `adaptor` field is a list. So you can declare as many adaptors as you want. In most cases, there are multiple adaptors per processor.

For an example, see the [ProSA Cargo.toml](https://github.com/worldline/ProSA/blob/main/prosa/Cargo.toml#L19).

You can also include your [processor declaration](ch03-02-declaration.md) in this metadata block if you declare your adaptor in the same crate as your processor.

This declaration step is very important because it simplifies the build process with [cargo-prosa](ch01-01-cargo-prosa.html#use).
