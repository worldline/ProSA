# OPS

This chapter is for people who want to know how to deploy a ProSA.

## ProSA architecture

Before deploying ProSA, you need to understand what it is.
ProSA is not a process on its own; It's a modular system that is fully customizable with a variety of processors and adaptors.

``` mermaid
flowchart LR
    proc1(Processor/Adaptor 1)
    proc2(Processor/Adaptor 2)
    proc3(Processor/Adaptor 3)
    procn(Processor/ADaptor N)
    prosa((ProSA))
    proc1 & proc3 <--> prosa
    prosa <--> proc2 & procn
```

A ProSA is useful with a set of processors and adaptors.

Every processor and its adaptor have a role, for example:
- Incoming HTTP server
- Outgoing Database
- etc,...

Every processor comunicates through an internal bus. This is better explained in the next [Adaptor chapter](ch02-00-adaptor.md).

Each processor and adaptor has its own configuration to define connection adresses, timers and so on.

With this "Lego" architecure, where you can include any processor that you need, you easily understand the necessity of having a tool to manage your ProSA instance structure.

This is the goal of Cargo-ProSA, the next part.
