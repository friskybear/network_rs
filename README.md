# Network_rs

Network_rs is a Python library that implements a simple libp2p request-response network protocol, originally developed in Rust.
> **Warning**
> **This project is experimental**: Be very careful here!
## Installation

To install the library, run the following command in your terminal:

```bash
pip install network_rs
```

*Note: It is recommended to run this command within a virtual environment.*

### Installing on Unsupported Platforms

If you encounter issues installing the library due to an unsupported operating system, you can install the Rust compiler on your device by running:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

After installing Rust, you can install the package using the same `pip` command. This will download the source distribution and build it for your platform.

For more information on installing Rust, visit the [official Rust website](https://www.rust-lang.org/learn/get-started).

## Usage Guide

Here is a simple example demonstrating how the functions in this library work:

```python
import network_rs as network
import asyncio
import json

# Custom function to be used in the network communication
def custom_function(broadcaster_inputs: str, receiver_inputs: list[str]):
    broadcaster_inputs_dict = json.loads(broadcaster_inputs)
    if broadcaster_inputs_dict['protocol'] == 'math':
        secret = int(broadcaster_inputs_dict['parameter']) * 10 * int(receiver_inputs[0])
        public = int(broadcaster_inputs_dict['parameter']) * 5
        return json.dumps({'protocol': 'math', 'public': public, 'secret': secret})

# Sender example
key = network.get_key()
port = network.get_free_port()
peer_id = network.get_peer_id(key)
addresses = []  # List of client addresses
broadcaster_inputs = {'protocol': 'math', 'parameter': '1'}
result = await network.send(key, port, json.dumps(broadcaster_inputs), addresses)
# Result: '{node_ip: {message: {'protocol': 'math', public: 5}}}'

# Receiver example
key = network.get_key()
port = network.get_free_port()
peer_id = network.get_peer_id(key)
receiver_inputs = ['10']  # Inputs given to custom_function from node
exclude = ['secret']  # Keys to exclude from the response sent back to broadcaster
n = 1  # Number of requests to receive
result = await network.receive(key, port, n, custom_function, receiver_inputs, exclude)
# Result: '{broadcaster_peer_id: {'protocol': 'math', 'public': 5, 'secret': 100}}'
```

## Running Tests

To run the tests, install the required Python dependencies and then start the broadcaster with the following command:

```bash
python ./broadcaster.py N Min Max Num_of_signatures
```

Where:
- `N` is the number of nodes you are running.
- `Min` is the minimum signature threshold (`Min <= Max <= N`).
- `Max` is the maximum number of people to make a signature (`Max <= N`).
- `Num_of_signatures` is the number of signatures you want.

Next, get a listening address from the output of the broadcaster command and use it to start the nodes:

```bash
python ./node.py address
```