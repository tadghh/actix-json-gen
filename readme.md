# Actix Data Generator

A primitive random data generator

- Can query with pretty print
- Supported sizes are KB to TB

```sh

curl "http://127.0.0.1:8080/generate?size=1500mb&format=json"

curl "http://127.0.0.1:8080/generate?size=100mb&format=csv"
```

- The `size` parameter specifies the target size of the generated content.
- The `format` parameter supports either JSON or CSV.
  - default: `json`

### Bugs

- Progress indicator may end up falling short.
