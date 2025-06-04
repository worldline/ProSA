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

Every processor and its adaptor have a role. For example:
- Incoming HTTP server
- Outgoing database
- Websocket client
- Etc.

Each processor and adaptor has its own configuration to define connection adresses, timers and so on.

Every processor communicates through an internal bus.
The goal of this bus is to facilitate transaction flow between processors with different routing configurations.
This will be better explained in the next [Adaptor chapter](ch02-00-adaptor.md).

With this "Lego" architecture, you can include any processor that you need and adapt message from one protocol to another as you wish.
Because a ProSA solution is deployed using multiple processors, we have created the Cargo-ProSA tool to help you orchestrate your solution.
We will cover this tool in the next section.
