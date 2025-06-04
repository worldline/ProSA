# Configuration

ProSA uses a standard YAML file to configure itself, as well as its sub-processors and adaptors.

## Single configuration file

By default, ProSA will look for the configuration file at `/etc/prosa.yml`.
The configuration is structured to include common settings and your desired processors:
```yaml
name: "prosa-name"
observability:
  level: debug
  metrics:
    stdout:
      level: info
  traces:
    stdout:
      level: debug
  logs:
    stdout:
      level: debug

proc-1:
  # Your processor 1 configuration

proc-2:
  # Your processor 2 configuration
```

## Multiple configuration file

You can also spread the configuration of your ProSA processors over multiple files.
Instead of specifing a single file, you can indicate a folder containing all your configuration files.

```yaml
# /etc/myprosa/main.yml

name: "prosa-name"
observability:
  level: debug
  metrics:
    stdout:
      level: info
  traces:
    stdout:
      level: debug
  logs:
    stdout:
      level: debug
```

```yaml
# /etc/myprosa/proc_1.yml

proc-1:
  # Your processor 1 configuration
```

```yaml
# /etc/myprosa/proc_2.yml

proc-1:
  # Your processor 2 configuration
```

## Environment variables

ProSA can also be configured using environment variables, assuming you only have one ProSA instance running on your system.
For example, you can set the ProSA name by filling the variable `PROSA_NAME="prosa-name"`.
