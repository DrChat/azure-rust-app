# Rocket ❤️ Azure
This is a basic example of using a [Rocket](https://rocket.rs/) app with [Azure app services](https://learn.microsoft.com/en-us/azure/app-service/overview).

## Quick Start
```
cargo run
```

## Quick Deployment
```
$RESOURCE_GROUP = "rustwebapp"
$DCR = "rustwebappdcr"
$APPSERVICE = "rustappservice"
$APP = "rustapp"

az group create --name $RESOURCE_GROUP --location southcentralus
az acr create -n $DCR -g $RESOURCE_GROUP --sku Standard --admin-enabled true
az acr login --name $DCR

docker build -t $DCR.azurecr.io/rustapp:latest .
docker push $DCR.azurecr.io/rustapp:latest

az appservice plan create -g $RESOURCE_GROUP -n $APPSERVICE --sku B1 --is-linux
az webapp create -g $RESOURCE_GROUP -p $APPSERVICE -n $APP -i $DCR.azurecr.io/rustapp:latest
az webapp config appsettings set -g $RESOURCE_GROUP -n $APP --settings WEBSITES_PORT=8000
```
