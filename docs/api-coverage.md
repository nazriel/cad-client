# API coverage

Status values: `implemented` is part of this PoC; `deferred` is intentionally
outside it. All paths below are relative to the selected application origin and
appear in its published OpenAPI document.

| # | Capability | Method | Path | Status |
|---:|---|---|---|---|
| 1 | Machine identity | GET | `/api/auth/v2/machine/identity` | implemented |
| 2 | List companies | GET | `/api/accounts/v1/companies` | implemented |
| 3 | List reporting periods | GET | `/api/reporting-periods/v1/reporting-periods` | implemented |
| 4 | Get reporting period | GET | `/api/reporting-periods/v1/reporting-periods/{id}` | implemented |
| 5 | Create reporting period | POST | `/api/reporting-periods/v1/reporting-periods` | implemented |
| 6 | Replace reporting period | PUT | `/api/reporting-periods/v1/reporting-periods/{id}` | implemented |
| 7 | Delete reporting period | DELETE | `/api/reporting-periods/v1/reporting-periods/{id}` | implemented |
| 8 | Duplicate reporting period | POST | `/api/reporting-periods/v1/reporting-periods/duplicate` | implemented |
| 9 | Get duplication status | GET | `/api/reporting-periods/v1/duplication-status/{id}` | implemented |
| 10 | List inventory lines | GET | `/api/measurements/v1/inventory-lines` | implemented |
| 11 | Get inventory line | GET | `/api/measurements/v1/inventory-lines/{id}` | implemented |
| 12 | Create inventory line | POST | `/api/measurements/v1/inventory-lines` | implemented |
| 13 | Patch inventory line | PATCH | `/api/measurements/v1/inventory-lines/{id}` | implemented |
| 14 | Delete inventory line | DELETE | `/api/measurements/v1/inventory-lines/{id}` | implemented |
| 15 | Prepare document upload | POST | `/api/documents/v1/pre-signed-write` | implemented |
| 16 | Prepare document download | GET | `/api/documents/v1/documents/{id}/pre-signed-read` | implemented |
| 17 | List custom fields | GET | `/api/tags/v1/fields` | implemented |
| 18 | Create custom field | POST | `/api/tags/v1/fields` | implemented |
| 19 | Replace custom field | PUT | `/api/tags/v1/fields/{id}` | implemented |
| 20 | Delete custom field | DELETE | `/api/tags/v1/fields/{id}` | implemented |
| 21 | List custom options | GET | `/api/tags/v1/options` | implemented |
| 22 | Create custom option | POST | `/api/tags/v1/options` | implemented |
| 23 | Batch-create custom options | POST | `/api/tags/v1/options/batch` | implemented |
| 24 | Replace custom option | PUT | `/api/tags/v1/options/{id}` | implemented |
| 25 | Delete custom option | DELETE | `/api/tags/v1/options/{id}` | implemented |
| 26 | List reports | GET | `/api/reports/v1/reports` | implemented |
| 27 | Get report progress | GET | `/api/reports/v1/reports/{id}` | implemented |
| 28 | Create report | POST | `/api/reports/v1/reports` | implemented |
| 29 | Read company benchmark | GET | `/api/benchmarks/v1/benchmarks` | implemented |
| 30 | Write company benchmark | PUT | `/api/benchmarks/v1/benchmarks/{id}` | implemented |

Routes not listed here are not implemented.
