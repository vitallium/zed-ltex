# LTeX+ Language Server

[LTEX+ Language Server](https://ltex-plus.github.io/ltex-plus/) support for Zed editor.

## Configuration

See the [LTeX+ Language Server documentation](https://ltex-plus.github.io/ltex-plus/settings.html) for more information on how to configure the LTeX+ Language Server.

### Changing the language

If you want to use a different language than the default, you can set the `ltex.language` setting to the desired language code. For example, to use German (Germany), set it to `de-DE`. See more information [here](https://ltex-plus.github.io/ltex-plus/settings.html#ltexlanguage).

```jsonc
{
  "lsp": {
    "ltex": {
      "settings": {
        "ltex": {
          "language": "de-DE"
        }
      }
    }
  }
}
```

### Add to dictionary, disable rule, hide false positive

These code actions write to files. Zed cannot run extension code when you pick a code action, so the extension asks LTeX+ LS to write the files itself (this needs LTeX+ LS 19.0.0 or later). List one file per language, with a `:` in front of the path:

```jsonc
{
  "lsp": {
    "ltex": {
      "settings": {
        "ltex": {
          "dictionary": { "en-US": [":~/.config/ltex/dictionary.en-US.txt"] },
          "disabledRules": { "en-US": [":~/.config/ltex/disabledRules.en-US.txt"] },
          "hiddenFalsePositives": { "en-US": [":~/.config/ltex/hiddenFalsePositives.en-US.txt"] }
        }
      }
    }
  }
}
```

- The path must be absolute or start with `~`.
- The file is created the first time a code action writes to it. It holds one entry per line, and its entries add to any entries you list in the setting itself.
- A code action is only offered when a file is listed for the language of the document. List the file under the language variant the document is checked in: with `"language": "en-GB"`, a file listed under `en-US` is not used.
- Changes to the files take effect at the next check; no restart is needed.

See [Server-Managed External Setting Files](https://ltex-plus.github.io/ltex-plus/ltex-ls-plus/server-usage.html#server-managed-external-setting-files) for details. To turn this off, set `ltex.externalFiles.managedByEditor` to `true` in `initialization_options`:

```jsonc
{
  "lsp": {
    "ltex": {
      "initialization_options": {
        "ltex": { "externalFiles": { "managedByEditor": true } }
      }
    }
  }
}
```
