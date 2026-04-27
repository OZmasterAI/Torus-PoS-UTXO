# TORUS

Ticker: TRS \
Proof of Stake: 5% yearly rate \
Min. stake age: 8 hours \
Block time: 120 sec

- Burn address: [TEuWjbJPZiuzbhuS6YFE5v4gGzkkt26HDJ](https://explorer.torus.cc/address/TEuWjbJPZiuzbhuS6YFE5v4gGzkkt26HDJ) \
  [See](contrib/burn-address.py) for more details.
- UBI Pool address:

---

## Run a node

Make sure to have a valid `TORUS.conf` file in `/home/$USER/.TORUS/TORUS.conf` or in any other path that was specified.
For more information about configuration file see [example](TORUS.conf).

Minimum `TORUS.conf` configuration file should include the following:

```bash
rpcuser=rpc
rpcpassword=password123
server=1
listen=1
```

#### Seed nodes

Official seed nodes:

- 95.111.231.121

Official DNS seed servers:

-

### Build from source

In order to build from source, check out [docs](doc). Specific dependencies can be found [here](doc/dependencies.md).

## Release notes

To see release notes check out this [file](doc/release-notes.md).
