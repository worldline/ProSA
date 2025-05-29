# Threads

For threading, ProSA relies on [Tokio](https://docs.rs/tokio/latest/tokio/runtime/index.html).

## Main threads

When you launch your ProSA, you have the option ` -t, --worker_threads <THREADS>` to run the main function with multiple threads.
By default, the main function will start observability tasks and the ProSA main task.

If you pay attention to the threads launched by ProSA for the main task, you'll see:

- For a single thread:
```bash
$ ps H -o 'flag,state,pid,ppid,pgid,pmem,rss,rsz,pcpu,time,cmd,comm' -p `pgrep -f prosa-dummy | head -1`
F S   PID  PPID  PGID %MEM   RSS   RSZ %CPU     TIME CMD                         COMMAND
0 S 26545  2698 26545  0.0  5488  5488  0.0 00:00:00 target/release/prosa-dummy  prosa-dummy
```

- For two threads (your program thread and 2 main threads):
```bash
$ ps H -o 'flag,state,pid,ppid,pgid,pmem,rss,rsz,pcpu,time,cmd,comm' -p `pgrep -f prosa-dummy | head -1`
F S   PID  PPID  PGID %MEM   RSS   RSZ %CPU     TIME CMD                         COMMAND
0 S 26591  2698 26591  0.0  7576  7576  0.0 00:00:00 target/release/prosa-dummy  prosa-dummy
1 S 26591  2698 26591  0.0  7576  7576  0.0 00:00:00 target/release/prosa-dummy  main
1 S 26591  2698 26591  0.0  7576  7576  0.0 00:00:00 target/release/prosa-dummy  main
```

## Processors threads

Processors, by default, use a [single-threaded Tokio runtime](https://docs.rs/tokio/latest/tokio/runtime/index.html#current-thread-runtime-behavior-at-the-time-of-writing).
Having a seperate runtime avoids any interference between processors.

Most of the time, having only one thread per processor is sufficient.
However, the behavior can be changed by implementing the [`get_proc_threads()`](https://docs.rs/prosa/latest/prosa/core/proc/trait.Proc.html#tymethod.get_proc_threads) method of the Proc trait.

This method return `1` indicating that your processor will spawn a runtime with a single thread.

If you wish for your processor to run on the main runtime, you can return `0`.

Finally, if you want to allocate multiple threads for your processor, you can return the desired number of threads to spawn from this method.
Of course, if you implement it, you can get the number of threads from your processor settings by adding a field for it.
