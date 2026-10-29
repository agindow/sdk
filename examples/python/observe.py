import json
import os
from urllib.request import urlopen
from agindow import Agindow

sdk = Agindow(mode="observe")
sdk.on("drift", lambda event: print("Drift:", event.verdict))

@sdk.guard(tool="orders.get", operation="read")
def get_order() -> object:
    with urlopen(os.environ["API_URL"], timeout=10) as response:
        order: object = json.load(response)
        return order

try:
    print("API result:", get_order())
    print("AGINDOW events:", [event.type for event in sdk.recent()])
finally:
    sdk.close()
