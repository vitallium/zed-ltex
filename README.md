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
