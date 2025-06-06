# Cloud

ProSA is intended to be cloud native/agnostic.
In this subsection, you'll find examples for deploying it on a Cloud PaaS[^paas].

Most of the time, there are PaaS offerings that work with Docker containers and Rust runtimes.

## Docker containers

To build a Docker image for your ProSA, refer to the [Cargo-ProSA Container](ch01-01-cargo-prosa.md#container)
Select a base image that suits your PaaS requirements and push the generated image to your cloud repository.

You'll find an example in the subsection for [GCP Cloud Run](ch01-04-01-gcp-cloud_run.md)

## Rust runtime

If your PaaS allows you to use the Rust runtime to run ProSA, you need to use the project generated by [Cargo-ProSA](ch01-01-cargo-prosa.html#use).

For an example, refer to the subsection for [Clever Cloud](ch01-04-02-clever_cloud.md)


[^paas]: Platform as a service - Run ProSA as a software without worrying about hardware, system, or infrastructure.
