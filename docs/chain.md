# The delegation chain's client

`bmo-chain` is how a unit asks the account service's delegation chain (the web console's `docs/accounts.md`,
「The delegation chain」): the platform, then the organisation's units, then the person, each link saying a kind's
value or handing it down. The account service keeps the chain and judges who may write a link; the client asks it
and reads what it answers, the same way in every unit.

## What it asks

- **A unit's chain** is the service's address, the service token, and the unit's own name, which every kind it
  declares and every value it writes names as the unit that declared it. A unit that names no address asks
  nothing, and `named` says so.
- **Its kinds are declared** with `declare`, each the unit's own body for it, `PUT /chain/kinds/{key}`, its
  `declaredBy` set to the unit's name. The first refusal or failure is the error.
- **A kind is resolved** for a person or a unit with `resolve`, `GET /chain/kinds/{key}/resolve`, and reads as one
  of three: resolved, with what the links said as the service answered it; undeclared, the service knowing no
  such kind; or nothing, the subject not standing on the chain. Any other failure is the error.
- **A link is read and written in a caller's name:** `entry`, `write_value`, `write_delegation` and `write_rules`
  carry the caller's key as `Owner-Key`, so the chain itself judges whether the caller reads or writes that link
  and whether a lock above withholds it; a value carries the unit's name as `Declared-By`. Each answers the
  service's status and body whole, a refusal among them, never an error of its own. `read` reads any other path of
  the service, in a caller's name where one is named.
- **What an answer holds** is read with `flag`, a resolved flag's value or the fallback the unit names, `units_of`,
  the units on the subject's path, farthest first, and `path_of`, every link of the subject's chain, top first.

## What it leaves to the unit

- **Which kinds it declares,** and their words.
- **What an undeclared kind or an unreachable service means:** a unit may declare its kinds again and ask once
  more, refuse, or go on as the chain had said nothing.
- **The value that stands where the chain says nothing,** which the unit hands `flag` as its fallback: a unit
  that guards what a person may do names the refusal, so nothing is allowed that the chain did not allow.
- The client keeps nothing between calls: every judgement asks afresh as it acts.
