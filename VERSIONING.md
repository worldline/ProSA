# Versioning

ProSA (and related projects) SHOULD use [Semantic Versioning 2.0.0](https://semver.org/) except for the cases described bellow.

## Early/unstable versions

In the case of **early/unstable** versions we recommend the following use:

- MAJOR version: always **0** which indicates **early/unstable** versions
- MINOR version: when you make **non** backward compatible changes
- PATCH version: when you add backward compatible changes and fixes

How do we justify this versioning scheme for **early/unstable** versions ?

When adding a dependency to a project we would do it like that:

- avoid declaring the PATCH version in the cargo.toml dependency to always use the latest patch.
- when you increment the MINOR version of such a dependency you know that it requires changes.

Example:

```toml
# MAJOR = 0 ==> early/unstable dependency
# MINOR changes indicates breaking changes
[workspace.dependencies]
prosa-utils = { version = "0.1", path = "prosa_utils" }
prosa-macros = { version = "0.1", path = "prosa_macros" }
bytes = "1" # stable (using Semantic Versioning 2.0.0)
```

## Stable released versions

Once your project releases a stable 1.x + version, it SHOULD use [Semantic Versioning 2.0.0](https://semver.org/)
