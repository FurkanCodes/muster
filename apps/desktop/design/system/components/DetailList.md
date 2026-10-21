# DetailList

Facts about one thing, as name and value: everything a plugin runs and needs, a ticket's source, a setting's file.

**Anatomy.** A `dl` (`mu-details`) in two columns: names in `text-quiet` as wide as the longest, values in `text` taking the rest, `space-8` between lines and `space-16` between columns, in `body`. Commands, paths and hashes go in `code` and wrap anywhere rather than being cut, since they must be read whole.

**Found or not.** A program a plugin needs reads ✓ `found` (`mu-found`, `done`) or ✗ `not found` (`mu-missing`, `waiting`), followed by how the plugin says to get it.

- Do: show a plugin everything it will run before it is approved, word for word.
- Don't: cut a command or a hash short.
