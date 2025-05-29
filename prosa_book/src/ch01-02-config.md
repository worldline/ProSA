# Configuration

ProSA uses a standard YAML file to configure itself, as well as its sub-processors and adaptors.

## Single configuration file

By default, ProSA will look for the configuration file at `/etc/prosa.yml`.
The configuration is structured to include common settings and your desired processors:
```yaml
name: "prosa-name"
observability:
  # See the next part

proc-1:
  # Your processor 1 configuration

proc-2:
  # Your processor 2 configuration
```

## Multiple configuration file

If you prefer to configure ProSA and your processors in seperate files, that's also possible.
Instead of specifing a single file, you can indicate a folder containing all your configuration files.

```yaml
# /etc/myprosa/main.yml

name: "prosa-name"
observability:
  # See the next part
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

If you want to see things up globally, you can also use environment variables.
For example, you can set the ProSA name filling the variable `PROSA_NAME="prosa-name"`.
