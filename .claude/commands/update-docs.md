# update-docs

Update the simpledb documentation to reflect recently added features.

## What this skill does

1. Reads the recently changed Rust source files to understand what was added or changed.
2. Updates the relevant markdown files under `docs/src/`.
3. Rebuilds the mdBook HTML (`docs/book/`) from the updated sources.
4. Commits and pushes both the markdown and HTML changes on the current branch.

---

## Writing style — the Feynman rule

Every doc you write must follow this standard:

**Explain it the way Richard Feynman would to a smart friend who has never seen this code.**

That means:
- Start with the *problem the code solves*, not the code itself.
- Use a concrete analogy before showing any code. The analogy should be something physical — a building directory, a post office, a library card catalogue.
- Never say "this method does X" without first saying *why anyone would want X*.
- When you show code, walk through it line by line the first time. Say what each line does *and why it is there*.
- If a concept is subtle (e.g. push-up vs copy-up in splits), explain it twice: once in plain English, once with a numbered step-by-step example using toy data.
- End every section with the "so what?" — what does this enable, what breaks if you get it wrong.
- No bullet lists of jargon. Full sentences. Short paragraphs. Lots of whitespace.
- Diagrams in ASCII where a picture beats a paragraph.

**If a 16-year-old curious about databases couldn't follow it, rewrite it.**

---

## Step-by-step instructions

### 1. Understand what changed

Run:
```
git diff main...HEAD --name-only -- 'src/**/*.rs'
```

Read each changed `.rs` file. For each one, identify:
- What new struct or method was added?
- What problem does it solve that didn't exist before?
- Which existing doc file is closest to this topic?

### 2. Identify which doc files need updating

The doc source lives in `docs/src/`. The mapping is:

| Code location | Doc file |
|---|---|
| `src/page/` | `docs/src/page/*.md` |
| `src/btree/leaf.rs`, `leaf_test.rs` | `docs/src/btree/leaf.md`, `docs/src/btree/split.md` |
| `src/btree/internal.rs`, `internal_test.rs` | `docs/src/btree/internal.md` |
| `src/btree/tree.rs` | new `docs/src/btree/tree.md` if it doesn't exist; add to `docs/src/SUMMARY.md` |
| `src/pager/` | `docs/src/pager.md` |
| Any completed milestone | `docs/src/roadmap.md` (mark ✅, remove the item from the todo list) |

### 3. Write or update each doc file

For each file:

- **New concept (new struct/type):** Add a new `##` section. Open with the real-world analogy, then the Rust type definition, then a walkthrough of the key methods.
- **New method on existing type:** Add a `##` section for the method. Show the signature, then walk through what it does step by step with a small example.
- **Split/merge behaviour:** Always include a full before/after ASCII diagram and a numbered step-by-step using toy keys like "alice", "bob", "carol".
- **Method reference table:** Keep it at the bottom of each page. Update it whenever a new method is added. Split the table by type if there are multiple structs (e.g. `InternalPage` vs `InternalPageMut`).
- **Roadmap:** Mark any completed item with ✅. If the item is now fully done, remove it from the todo list and add one sentence in past tense at the top of the section: *"As of [feature], X is complete."*

### 4. Rebuild the book

```
cd docs && mdbook build
```

Check there are no errors in the output.

### 5. Commit and push

Stage both source and built output:
```
git add docs/src/ docs/book/
git commit -m "Update docs: <short summary of what changed>"
git push
```

---

## Quality checklist before committing

- [ ] Every new public struct has a section in the relevant doc file.
- [ ] Every new public method is in the method reference table.
- [ ] The writing uses at least one analogy per new concept.
- [ ] Every split/complex algorithm has a before/after ASCII diagram.
- [ ] `docs/src/roadmap.md` reflects the current state (✅ for done items).
- [ ] `mdbook build` completed with no errors.
- [ ] No doc section says "this method does X" without first saying *why X is needed*.
