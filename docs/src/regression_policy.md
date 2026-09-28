# Regression policy: decide what counts as a performance regression

> **Note:** hotpath Cloud is currently in closed beta.

The [hotpath Cloud](cloud.md) policy is the TOML document that decides which changes between a pull request and its baseline count as regressions. This page documents where the policy lives and how a run picks it. The reference of the document itself will follow.

## The policy is a file in your repository

A policy is a TOML file that you commit next to your code. A profiled run reads the file from the checkout it measured and sends it inside its report, exactly as written, comments included. The server judges each report under the policy that report carried.

Two consequences follow from that:

- A pull request is judged under its own policy. When a pull request edits the policy file, its report is judged under the edit, and no other report is.
- The judgement of a stored report never changes. Editing the policy later affects the reports uploaded after the edit, not the ones already stored.

A report without a policy is judged under the built-in default.

## Lookup order

A run picks the first file that exists, in this order:

1. The file that the `HOTPATH_POLICY_PATH` environment variable names.
2. `hotpath/<benchmark>-policy.toml`, where `<benchmark>` is the value of `HOTPATH_BENCHMARK`.
3. `hotpath/policy.toml`, shared by every benchmark of the repository.

Without any of them the report carries no policy. Paths 2 and 3 are relative to the root of the git repository.

Exactly one document judges a report. Files are not merged: a benchmark with its own file does not inherit from the shared one, and any key that the chosen document omits takes the built-in value.

`HOTPATH_POLICY_PATH` is relative to the working directory, or absolute. It must resolve inside the repository, because a report names its policy by the path relative to the repository root. A file outside the repository is never sent.

## Files that cannot be sent

A policy file is refused when it is unreadable, not valid UTF-8, blank, larger than 65536 bytes, or outside the repository (symbolic links are followed before that check). A refused file is never treated as missing: the run does not fall back to the next file in the order. It prints one line that names the file and the reason, and writes its report without a policy:

```
hotpath: the policy file `hotpath/ci-policy.toml` is blank. The report carries no policy.
```

The profiled program never parses the policy. A file that can be sent but is not a valid policy is found by the server: the report is stored and judged under the built-in default, and the upload says so with a warning next to the verdict.

## Checking a policy before it is used

`hotpath cloud validate-policy` asks the server whether the policy files of a checkout are valid, with the same lookup a run uses:

```bash
# Every policy file in hotpath/: policy.toml and each *-policy.toml
hotpath cloud validate-policy

# The one file a run of this benchmark picks
hotpath cloud validate-policy --benchmark ci

# One file, wherever it is ("-" reads stdin)
hotpath cloud validate-policy --file candidate.toml
```

The command needs the `cloud` feature of the `hotpath` binary and an API token in `HOTPATH_API_TOKEN`. It prints a JSON list with one entry per file checked:

```json
[
  {
    "path": "hotpath/ci-policy.toml",
    "valid": false,
    "problems": [
      { "line": 2, "message": "unknown key `functions.timing.min_percent`" }
    ]
  },
  { "path": "hotpath/policy.toml", "valid": true, "problems": [] }
]
```

The exit code is `0` when every file checked is valid, `1` when a file is not valid or no policy file was found, and `2` for a usage error. One run checks at most 64 files.

## Which policy judged a report

A stored report names its policy instead of returning it:

- `policy_path` is the path of the policy file relative to the repository root, for example `hotpath/ci-policy.toml`.
- `policy_url` is that file on GitHub, pinned to the measured commit, so it shows the document that judged the report whatever the branch holds now.

Both are `null` only when the report carried no policy and the built-in default judged it.
