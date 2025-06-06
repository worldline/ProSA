# Processor

This chapter is intended for advanced developers who want to build their own ProSA processors.

A processor is a central part of ProSA. It's used to handle the main processing logic.

``` mermaid
flowchart LR
    ext(External System)
    adapt(Adaptor)
    tvf(TVF)
    proc(<b>Processor</b>)
    settings(Settings)
    ext <-. Protocol Exchange .-> proc
    adapt <-- internal communication --> tvf
    settings --> Processor
    subgraph Processor
    proc <-- protocol adaptation --> adapt
    end
```

There are several kinds of processors:

- _Protocol_ - Used to handle a specific protocol and map it to internal TVF messages.
- _Internal_ - Handles only internal messages; useful for modifying or routing messages.
- _Standalone_ - The processor works independently, with no internal messages involved; useful for interacting with external systems.
