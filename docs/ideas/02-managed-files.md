# Managed files

Status: Parked

## Executive summary

Allow psst to manage files alongside secrets. These files are usually untracked, environment-related configuration files retrieved from a specified source and written to a specified filesystem destination. A command such as `psst write-files` would materialize them.

## Notes

- Specify a source and destination for each managed file, analogous to specifying a source for a secret.
- Files may be fetched from different sources; the supported sources are not yet defined.
- In the future, support the reverse operation: update a source from a locally edited file where the source supports writes. Some sources are read-only.
- The first-iteration scope remains unspecified: the original description ended at “in our first iteration”.
- Terminology is unresolved: the current glossary defines providers, entries, references, and configuration in terms of secrets and environment variables. How managed files relate to those concepts requires discussion.
