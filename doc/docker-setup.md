# Docker Setup (TODO — update image registry)

Torus full node can be run using a prebuild Docker image. This is recommended as the image is under the active development. \
Image tagged as `latest` will always match the master branch.
If you want to run a stable release, use the image tag that corresponds to the official release - e.g. `torusd:1.0.0`.

Pull image:

```bash
docker pull ghcr.io/torus-economy/torusd:latest
```

Run container:

```bash
docker run \
    -d \
    -p 24111:24111 \
    -v /home/$USER/.TORUS:/root/.TORUS \
    --name TORUSd \
    --restart=always \
    ghcr.io/torus-economy/torusd:latest
```

or with RPC port enabled:

```bash
docker run \
    -d \
    -p 24111:24111 \
    -p 24112:24112 \
    -v /home/$USER/.TORUS:/root/.TORUS \
    --name TORUSd \
    --restart=always \
    ghcr.io/torus-economy/torusd:latest
```

Make sure to have a valid `TORUS.conf` file in `/home/$USER/.TORUS/TORUS.conf` or in any other path that was specified.
For more information about configuration file see [example](TORUS.conf).
Docker container must always have torusd process running in the foreground, so do not include `daemon=1` in `TORUS.conf` configuration file when running within Docker.
In case `daemon=1` is included, the Docker process will exit immediately.

Minimum `TORUS.conf` configuration file should include the following:

```bash
rpcuser=rpc
rpcpassword=password123
server=1
listen=1
```

## docker-compose

This could also be achieved by running a docker-compose script.
Preconfigured docker-compose script with corresponding `TORUS.conf` configuration file can be found in [contrib](contrib/docker-compose) folder.
For security reasons, make sure to change `rpcuser` and `rpcpassword` default values.
Afterwards, the script can be run:

```bash
cd contrib/docker-compose
docker-compose up -d
```

## TORUSd daemon commands in Docker

If TORUSd is running in Docker, daemon commands can be run in the following way:

```bash
docker exec TORUSd ./TORUSd <command> <params>
```

For example, to get basic info and staking info:

```bash
docker exec TORUSd ./TORUSd getinfo
docker exec TORUSd ./TORUSd getstakinginfo
```
