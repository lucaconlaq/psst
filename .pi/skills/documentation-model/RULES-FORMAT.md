# Rules Format

Keep rule documents in `docs/rules/` and register their file patterns and scope in
`docs/rules/index.md`. This format applies independently of the languages or rule
documents present in a repository.

## Structure

- Start with a descriptive level-one title and a generic introduction stating scope.
- Put external references in the introduction as context and fallback for details
  not covered locally; do not repeat source citations in every rule.
- Follow the introduction with numbered level-two headings in the form
  `## <PREFIX>-<N>: <Title>`, for example `## RUST-1: Formatting foundations`.
- Use the document's uppercase name as the prefix (`RUST.md` → `RUST`) and number
  rules sequentially starting at 1.
- Keep related guidance, examples, tables, and checklists under the relevant rule;
  use level-three headings for subsections as needed.
- Refer to rules by their identifiers, such as `RUST-9`.
- Distinguish repository-specific requirements from external guidance when needed.

When reformatting existing guidelines, preserve their requirements and meaning;
change only presentation, citation placement, and references needed by the format.
