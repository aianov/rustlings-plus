# Сервисов два, api и auth. Запускать нужно любой из них, и хочется писать так:
#   make run                    запускает api (сервис по умолчанию)
#   make run SERVICE=auth       запускает auth
#   SERVICE=auth make run       то же самое через окружение
#   make run PROFILE=release    запускает в release-режиме
# Сейчас запускается только api, и менять это приходится в файле.
SERVICE = api

run:
	cargo run --quiet -p api
