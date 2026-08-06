# robrowser-remoteclient

Serves the assets of a Ragnarok Online client over HTTP, for use as a remote
client by [roBrowser](https://github.com/MrAntares/roBrowserLegacy).

Files are looked up in the loose directories (`data`, `BGM`, `System`, `AI`)
first, then in the GRF archives listed in `DATA.INI`, in priority order.
Lookups are case-insensitive and accept both `/` and `\` separators.

## Usage

```sh
robrowser-remoteclient /path/to/client
```

| Option        | Environment                     | Default        |
| ------------- | ------------------------------- | -------------- |
| `<client>`    | —                               | required       |
| `--bind ADDR` | `ROBROWSER_REMOTECLIENT_BIND`   | `0.0.0.0:8080` |
| `--cors`      | `ROBROWSER_REMOTECLIENT_CORS`   | off            |

## Docker

```sh
docker run --rm -p 8080:8080 -v /path/to/client:/client:ro \
  ghcr.io/ubermanu/robrowser-remoteclient
```

Or as a Docker Compose service:

```yaml
services:
  remoteclient:
    image: ghcr.io/ubermanu/robrowser-remoteclient
    volumes:
      - ./client:/client:ro
    ports:
      - "8080:8080"
    environment:
      ROBROWSER_REMOTECLIENT_CORS: "true"
```

## Build

```sh
cargo build --release
```
