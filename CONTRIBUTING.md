# Contributing Guide

ProSA projects accept contributions via GitHub pull requests. This document outlines the process to help get your contribution accepted.

## How to Contribute Code

1. Identify or create the related issue.
2. Fork the desired repo; develop and test your code changes.
3. Submit a pull request, making sure to [sign your work](#developer-certificate-of-origin) and link the related issue.

In general, ProSA project must follow [Rust coding standards](https://doc.rust-lang.org/nightly/style-guide/) when you're contributing.

## Pull Requests

We use Pull Requests (PRs) to track code changes.

## Developer Certificate of Origin

As with other CNCF projects, ProSA has adopted a [Developers Certificate of Origin (DCO)](https://developercertificate.org/). A DCO is a lightweight way for a developer to certify that they wrote or otherwise have the right to submit code or documentation to a project.

The sign-off is a simple line at the end of the explanation for a commit. All commits need to be
signed. Your signature certifies that you wrote the patch or otherwise have the right to contribute
the material. The rules are pretty simple, if you can certify the below (from
[developercertificate.org](https://developercertificate.org/)):

```
Developer Certificate of Origin
Version 1.1

Copyright (C) 2004, 2006 The Linux Foundation and its contributors.

Everyone is permitted to copy and distribute verbatim copies of this
license document, but changing it is not allowed.


Developer's Certificate of Origin 1.1

By making a contribution to this project, I certify that:

(a) The contribution was created in whole or in part by me and I
    have the right to submit it under the open source license
    indicated in the file; or

(b) The contribution is based upon previous work that, to the best
    of my knowledge, is covered under an appropriate open source
    license and I have the right under that license to submit that
    work with modifications, whether created in whole or in part
    by me, under the same open source license (unless I am
    permitted to submit under a different license), as indicated
    in the file; or

(c) The contribution was provided directly to me by some other
    person who certified (a), (b) or (c) and I have not modified
    it.

(d) I understand and agree that this project and the contribution
    are public and that a record of the contribution (including all
    personal information I submit with it, including my sign-off) is
    maintained indefinitely and may be redistributed consistent with
    this project or the open source license(s) involved.
```

Then you just add a line to every git commit message:

    Signed-off-by: Joe Smith <joe.smith@worldline.com>

Use your real name (no pseudonyms or anonymous contributions)

If you set your `user.name` and `user.email` git configs, you can sign your commit automatically
with `git commit -s`.

Note: If your git config information is set properly then viewing the `git log` information for your
 commit will look something like this:

```
Author: Joe Smith <joe.smith@worldline.com>
Date:   Thu May 12 15:21:43 2024 +0100
    Update documentation
    Signed-off-by: Joe Smith <joe.smith@worldline.com>
```

Notice the `Author` and `Signed-off-by` lines match. If they don't your PR will be rejected by the
automated DCO check.

- In case you forgot to add it to the most recent commit, use `git commit --amend --signoff`
- In case you forgot to add it to the last N commits in your branch, use `git rebase --signoff HEAD~N` and replace N with the number of new commits you created in your branch.
- If you have already pushed your branch to a remote, will need to push your changes to overwrite the branch: `git push --force-with-lease origin my-branch`

## Issues

Issues are used as the primary method to track anything related to ProSA projects.

### Issue lifetime

1. When creating an issue, make sure to use correct labeling.
2. A maintainer or contributor will sort the issue, to be processed. If additionnal levels are needed, we will add them.
3. Issue will be discuss depending of the request, and will be implemented or closed
