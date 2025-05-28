# Adaptor

This chapter is for application developers who want to know how to build an application using existing ProSA processors.

This chapter will be the most abtract of all, as the Adaptor's implementation is up to the processor developer.
However, developers need to follow certain guidelines, which will be outlined here.
These guidelines are also useful for processor developers to ensure they expose their processor effectively.

Adaptors serve as type boundaries that application developers should not cross.
They are designed to provide a simple interface for those who may not be familiar with protocols.
You know what processing needs to be done on a specific message, but not the underlying protocol that transports it.

## Relation

Adaptors are related to a processor.
They are called by the processor, so they need to implement all the interfaces the processor requires; otherwise, they won't function properly.

A processor can have only one adaptor when running.

``` mermaid
flowchart LR
    ext(External System)
    adapt(<b>Adaptor</b>)
    proc(Processor)
    ext <-- Protocol Exchange --> adapt
    subgraph Processor
    adapt <-- protocol adaptation --> proc
    end
```

The adaptor should be viewed as an interface between the internal ProSA TVF messaging system and the external connected system.
