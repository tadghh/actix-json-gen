# Actix JSON Generator

A primative random JSON generator

- Can query with pretty print
- Supported sizes are KB to TB

```sh
curl "http://127.0.0.1:8080/generate?size=100mb&pretty=true"

curl "http://127.0.0.1:8080/generate?size=1500mb"
```

- The `pretty` parameter enables pretty-printed output (optional).
- The `size` parameter specifies the target size of the generated content.
