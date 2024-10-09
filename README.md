# LTeX Language Server

[LTEX Language Server](https://github.com/valentjn/ltex-ls) support for Zed editor

## Configuration

See the [LTeX Language Server documentation](https://valentjn.github.io/ltex/settings.html) for more information on how to configure the LTeX Language Server.

### Changing the language

If you want to use a different language than the default, you can set the
`ltex.language` setting to the language code you want to use. For example, to
use German, you would set the setting to `de`.

```jsonc
{
  "lsp": {
    "ltex": {
      "settings": {
        "ltex": {
          "language": "de"
        }
      }
    }
  }
}
```
