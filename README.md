# ipp-proxy

> Heavily inspired by https://github.com/tuupke/pixie

This is an IPP proxy that receives print jobs and forwards them to an upstream printer.

This proxy is made for SWERC (or ICPC-like competitions), where there are teams that want to print but on each sheet of paper there should be an indication of the original team (so that the papers can be delivered to the correct table).
The PDFs to print will be modified adding an overlay indicating which team the request comes from, as well as the location of the team (i.e. which desk), the number of pages and additional information.

The printer to use can be customized on a team-by-team basis.
The teams are logged in either using the request IP, or by providing the team's password in the IPP URI.
The configuration is set in the database.

## Setup

To setup the proxy you'll need:

- CUPS installed on each host that is connected to a printer
- ipp-proxy running on a single host reachable from all the clients, and that can reach all the hosts with a printer
- CUPS installed on all the clients

### Setup the printer on the print host

Install CUPS on each host that is connected to a printer, and start the cups service.
From the admin interface (at http://localhost:631), add the printer.

> For testing you can add a cups-pdf printer (that prints to a PDF file).
> Install cups-pdf and add the printer using `CUPS-PDF (Virtual PDF Printer)`

Remember the _name_ of the printer, as it will be used to configure the upstream in the proxy.
Select also _Share this printer_, to make it available to the proxy from the network.

### Setup the proxy

Create an empty database with:

```shell
sqlite3 db.sqlite3 < schema.sql
```

Compile the proxy using:

```shell
DATABASE_URL=sqlite:./db.sqlite3 cargo build --release
```

Start the proxy with:

```shell
RUST_LOG=ipp_proxy=debug,actix_web=info cargo run --release
```

Now you can add the teams to the database.
You can use the tool you prefer to interact with the SQLite database.

When you enter the values for `ipp_upstream` for a team use this format:

```
<ip-of-print-host>:631/printers/<name-of-printer>
```

### Setup the clients

Install CUPS on the client PCs.

Add a new printer with the following settings:

- Other Network Printers: Internet Printing Protocol (ipp)
- If you want to use IP authentication:
  - Connection: `ipp://<ip-of-proxy>:6632/`
- If you want to use password authentication:
  - Connection: `ipp://<ip-of-proxy>:6632/password=<password>/`
- Name: you can choose it, it will be shown in the print menu
- Make: `Generic`
- Model: `Generic PDF Printer`

## Example output

![image](https://user-images.githubusercontent.com/6685454/163812355-1fe28d4b-e1f9-4efa-9cb1-13b615464da6.png)
