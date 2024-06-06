import sys
import network_rs
import asyncio
import json
from frost_rs import utility_secp256k1 as frost

def round1(inputs: str, _id: list[str]) -> str:
    inputs = json.loads(inputs)
    if inputs["protocol"] == 'round1':
        (round1_secret_package, round1_public_package) = frost.round1(
            _id[0], int(inputs["min_signers"]), int(inputs["max_signers"]))
        return json.dumps({'protocol': 'round1',  'status': 'success', 'id': _id[0], 'round1_public_package': round1_public_package, 'round1_secret_package': round1_secret_package})
    else:
        return json.dumps({'protocol': 'round1', 'status': 'Failed'})


def round2(inputs: str, round1_secrets: list[str]):
    inputs = json.loads(inputs)
    round1_received_packages: dict[str:str] = {k: v for k, v in json.loads(
        inputs['round1_received_packages']).items() if k != round1_secrets[1]}

    if inputs["protocol"] == 'round2':
        (round2_secret_package, round2_public_package) = frost.round2(
            round1_secrets[0], round1_received_packages)
        return json.dumps({'protocol': 'round2', 'round1_received_packages': round1_received_packages, 'status': 'success', 'id': round1_secrets[1], 'round2_public_package': json.dumps(round2_public_package), 'round2_secret_package': round2_secret_package})
    else:
        return json.dumps({'protocol': 'round2', 'status': 'Failed'})


def round3(inputs: str, round3_requirement: list[str]):
    inputs = json.loads(inputs)

    round1_received_packages: dict[str:str] = {k: v for k, v in json.loads(
        round3_requirement[1]).items() if k != round3_requirement[2]}

    round2_secret_package: str = round3_requirement[0]
    my_round2_packages = {k: json.loads(v)[round3_requirement[2]] for k, v in
                          inputs['round2_received_packages'].items() if round3_requirement[2] in v}
    if inputs["protocol"] == 'round3':
        (key_package, public_key) = frost.round3(
            round2_secret_package, round1_received_packages, my_round2_packages)

        return json.dumps({'protocol': 'round3',  'status': 'success', 'id': round3_requirement[2], 'public_key': json.dumps(public_key), 'key_package': key_package})
    else:
        return json.dumps({'protocol': 'round3', 'status': 'Failed'})


def generate_nonce(inputs: str, package: list[str]):
    inputs = json.loads(inputs)
    if inputs["protocol"] == 'generate_nonce':
        nonces = []
        commitments = []
        num_of_nonces = inputs['num_of_nonces']
        for _ in range(0, num_of_nonces):
            (nonce, commitment) = frost.preprocess(package[0])
            nonces.append(nonce)
            commitments.append(commitment)
        return json.dumps({'protocol': 'generate_nonce', 'status': 'success', 'public_nonces': commitments, 'secret_nonces': nonces})
    else:
        return json.dumps({'protocol': 'generate_nonce', 'status': 'Failed'})


def sign(inputs: str, package: list[str]):
    inputs = json.loads(inputs)
    message = inputs['message']
    commitments = inputs['commitments']
    nonce = json.loads(package[1])[commitments[package[2]]]
    key_package = package[0]
    if inputs["protocol"] == 'sign':

        share = frost.sign(bytes(message, 'utf-8'),
                           commitments, nonce, key_package)
        return json.dumps({'protocol': 'sign', 'message': message, 'status': 'success', 'share': share})
    else:
        return json.dumps({'protocol': 'sign', 'status': 'Failed'})


async def main():
    ip = sys.argv[1]
    key = network_rs.get_key()
    port = network_rs.get_free_port()
    peer_id: str = network_rs.get_peer_id(key)
    _id = frost.get_id(peer_id)
    protocol = {'protocol': 'coordinate'}
    result = await network_rs.send(key, port, json.dumps(protocol), [ip])
    result: str = await network_rs.receive(key, port, 1, round1, [_id], ["round1_secret_package"])
    result_dict: dict = json.loads(result)

    round1_secret_package: str = list(result_dict.values())[
        0]['round1_secret_package']
    result: str = await network_rs.receive(key, port, 1, round2, [round1_secret_package, _id], ["round2_secret_package"])
    result_dict: dict = json.loads(result)
    round1_received_packages: dict = list(result_dict.values())[
        0]['round1_received_packages']
    round2_secret_package = str = list(result_dict.values())[
        0]['round2_secret_package']
    result: str = await network_rs.receive(key, port, 1, round3, [round2_secret_package, json.dumps(round1_received_packages), _id], ['key_package'])
    public_key = list(json.loads(result).values())[0]['public_key']
    key_package = list(json.loads(result).values())[0]['key_package']
    result: str = await network_rs.receive(key, port, 1, generate_nonce, [key_package], ['secret_nonces'])
    result_dict = list(json.loads(result).values())[0]
    nonces = dict(zip(result_dict['public_nonces'],
                  result_dict['secret_nonces']))
    num_of_sig = len(nonces)

    result = await network_rs.receive(key, port, num_of_sig, sign, [key_package, json.dumps(nonces), _id], [])
    exit(0)

asyncio.run(main())
