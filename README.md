# OpenFaaS Functions OperatoRS

## Install

* Install the CRD
```bash
openfaas_functions_operato_rs crd install
```

* Install the Operator in controller mode
```bash
openfaas_functions_operato_rs operator controller deploy install
```

* Now you can create a Function and wait for it to be ready
```bash
kubectl apply -f nodeinfo.yaml
kubectl wait --for=condition=ready openfaasfunctions -n openfaas-fn nodeinfo
```

## Run locally

* Run the Operator in controller mode
```bash
openfaas_functions_operato_rs operator controller run
```

## Uninstall

* Uninstall the CRD
```bash
openfaas_functions_operato_rs crd uninstall
```

* Uninstall the Operator
```bash
openfaas_functions_operato_rs operator controller deploy uninstall
```

* Delete a Function
```bash
kubectl delete -f nodeinfo.yaml
```

## Help

```bash
openfaas_functions_operato_rs --help
```

## Notes

* The Function CRD is based on the ```OpenFaaS Function CRD``` with optional fields

## Rust version 

1.70.0

## Contributors

* Jad K. Haddad <jadkhaddad@gmail.com>

## License & copyright

© 2023 Jad K. Haddad
Licensed under the [Creative Commons Legal Code License](LICENSE)