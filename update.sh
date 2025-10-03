#!/bin/bash
curl https://hanabi.so/api/sandwiching_report -F "filtered_report=@reports/$1/filtered_report.csv" -F "report_path=$1" > README.md
