# psst Glossary

Canonical domain vocabulary for code, documentation, and discussions.

## Terms

**Secret**:
A value managed by psst for use in an environment variable. A secret may be retrieved through a provider or stored as a manual literal.
_Avoid_: Entry (when referring to the resolved value)

**Entry**:
A configuration item that associates an environment name with a provider and a stored value. The stored value is a provider reference, a shell command, or a manual literal—not necessarily the resolved secret.
_Avoid_: Secret (when referring to the configuration item)

**Provider**:
A source of secrets that interprets an entry's stored value to obtain its secret, including manual values and shell commands.

**Reference**:
A locator identifying a secret within a provider, such as a 1Password field or Terraform output. Manual literals and shell commands are stored values, not locators.

**Environment name**:
The name of the environment variable associated with an entry and used when injecting its secret.

**Local storage ID**:
An identifier for an encrypted secret in local storage, independent of its environment name. Together with the public key, it identifies a stored record; the same ID can identify different secrets under different keys.

**Configuration**:
The collection of entries defining which secrets psst manages and their associated environment names.
