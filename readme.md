# Actix Data Generator

A primative random data generator

- Can query with pretty print
- Supported sizes are KB to TB

```sh
curl "http://127.0.0.1:8080/generate?size=100mb&pretty=true&format=json"

curl "http://127.0.0.1:8080/generate?size=1500mb&format=json"

curl "http://127.0.0.1:8080/generate?size=100mb&format=csv"
```

- The `pretty` parameter enables pretty-printed output (optional).
  - default: `false`
- The `size` parameter specifies the target size of the generated content.
- The `format` parameter supports either JSON or CSV.
  - default: `json`

### Bugs

- Progress indicator may end up falling short.
