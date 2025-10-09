# VCF MCP Server: Exposing Varient Calling Format files to LLMs for Analysis

## Overview

This repository contains the code for the VCF MCP Server, which exposes variant calling format (VCF) files to large language models (LLMs) for analysis. This server is built using Rust.

Inputs: one VCF file and optionally one TBI index file.
MCP functionality: It supports looking up variants in by chromosome, position, and ID.
