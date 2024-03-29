# Axum ❤️ Azure
This is a basic example of using a [Axum](https://github.com/tokio-rs/axum) app with [Azure app services](https://learn.microsoft.com/en-us/azure/app-service/overview).

## Quick Start
```
cargo run
```

## Quick Deployment
```
New-AzResourceGroup -Name "webapp" -Location "South Central US"
New-AzResourceGroupDeployment -ResourceGroupName "webapp" -TemplateFile .\deployment.bicep -TemplateParameterObject @{ `
  webAppName = "testapp"
}

Connect-AzContainerRegistry -Name <insert name of ACR resource here>
docker build -t <ACR>.azurecr.io/testapp:latest .
docker push <ACR>.azurecr.io/testapp:latest
```

## Manual Deployment
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
