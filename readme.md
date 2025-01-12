# Actix JSON Generator

A very primative random JSON generator

- Can query with pretty print
- Supported sizes are KB to TB

```sh
curl "http://127.0.0.1:8080/generate?size=100mb&pretty=true"
```

Takes 13 seconds to generate 2GB of random data on Dual Xeon Gold 5122. Mainly unoptimized string operations and we lock the thread every 200 iterations to update the progress bar :) (very slow hehe, but also sorta not?)
