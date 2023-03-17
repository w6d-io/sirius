#!/bin/bash

if [ -f ".gitconfig" ]
then
	echo "copy git config"
	cp .gitconfig ~
fi

if [ -f ".git-credentials" ]
then
	echo "copy git credentials"
	cp .git-credentials ~
fi
