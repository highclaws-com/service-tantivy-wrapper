#!/bin/bash
set -e

PORT=${1:-8080}
API_URL="http://127.0.0.1:${PORT}"

echo "=========================================="
echo "1. Indexing test documents one by one"
echo "=========================================="

echo "Indexing README.md..."
curl -s -X POST "$API_URL/index_docs" -H "Content-Type: application/json" -d '{
	"docs": [{
		"hash": "README.md",
		"text": "Welcome to the search engine project README. This project uses docker for containerization and wopi for document integrations.",
		"tags": ["readme", "ext_md"]
	}]
}'
echo ""

echo "Indexing wopi_setup.md..."
curl -s -X POST "$API_URL/index_docs" -H "Content-Type: application/json" -d '{
	"docs": [{
		"hash": "wopi_setup.md",
		"text": "WOPI docker setup instructions. The wopi protocol is heavily used for office document editing in our stack.",
		"tags": ["wopi", "ext_md"]
	}]
}'
echo ""

echo "Indexing random_notes.txt..."
curl -s -X POST "$API_URL/index_docs" -H "Content-Type: application/json" -d '{
	"docs": [{
		"hash": "random_notes.txt",
		"text": "Just some random notes about server maintenance, security, and wopi.",
		"tags": ["notes", "ext_txt"]
	}]
}'
echo ""

echo ""
echo "=========================================="
echo "2. Searching for keyword 'dockers' to test stemming"
echo "=========================================="
# Should match README.md, docker_guide.md, and random_notes.txt
curl -s -X POST "$API_URL/search" -H "Content-Type: application/json" -d \
	'{"q": "dockers", "limit": 3}' | jq .

echo ""
echo "=========================================="
echo "3. Searching for keyword 'docker' with one tag 'readme'"
echo "=========================================="
# Should ONLY match docker_guide.md
curl -s -X POST "$API_URL/search" -H "Content-Type: application/json" -d \
	'{"q": "docker", "tags": ["readme"], "limit": 3}' | jq .

echo ""
echo "=========================================="
echo "3. Searching for keyword 'docker' with tags 'ext_md' and 'wopi'"
echo "=========================================="
curl -s -X POST "$API_URL/search" -H "Content-Type: application/json" -d \
	'{"q": "docker", "tags": ["ext_md", "wopi"], "limit": 3}' | jq .

echo ""
echo "=========================================="
echo "5. Searching for keyword 'docker' with invalid tag"
echo "=========================================="
curl -s -X POST "$API_URL/search" -H "Content-Type: application/json" -d \
	'{"q": "docker", "tags": ["non-existent", "wopi"], "limit": 3}' | jq .

echo ""
echo "=========================================="
echo "8. Searching 'docker' OR 'wopi' (space-separated, implicit OR)"
echo "=========================================="
curl -s -X POST "$API_URL/search" -H "Content-Type: application/json" -d \
	'{"q": "docker wopi", "limit": 5}' | jq .

echo ""
echo "=========================================="
echo "9. Searching 'docker' AND 'wopi' "
echo "=========================================="
curl -s -X POST "$API_URL/search" -H "Content-Type: application/json" -d \
	'{"q": "docker AND wopi", "limit": 5}' | jq .

echo ""
echo "=========================================="
echo "9. Searching 'docker' AND 'wopi' with smaller snippets"
echo "=========================================="
curl -s -X POST "$API_URL/search" -H "Content-Type: application/json" -d \
	'{"q": "docker AND wopi", "limit": 5, "snippet_length": 30 }' | jq .

echo ""
echo "=========================================="
echo "10. Retrieving documents (README.md)"
echo "=========================================="
curl -s -X POST "$API_URL/docs" \
	-H "Content-Type: application/json" \
	-d '{"hashes": ["README.md"]}' | jq .

echo ""
echo "=========================================="
echo "11. Deleting README.md"
echo "=========================================="
curl -s -X POST "$API_URL/delete" \
	-H "Content-Type: application/json" \
	-d '{"hashes": ["README.md"]}'
echo ""

echo ""
echo "=========================================="
echo "12. Searching again to verify deletion"
echo "=========================================="
curl -s -X POST "$API_URL/docs" \
	-H "Content-Type: application/json" \
	-d '{"hashes": ["README.md"]}' | jq .

echo ""
echo "=========================================="
echo "13. Testing Commit Delay (commit=false)"
echo "=========================================="
curl -s -X POST "$API_URL/index_docs" -H "Content-Type: application/json" -d '{
	"commit": false,
	"docs": [{
		"hash": "delayed_doc.md",
		"text": "This document is delayed."
	}]
}'

echo "Searching for 'delayed' (should be empty)..."
curl -s -X POST "$API_URL/search" -H "Content-Type: application/json" -d '{"q": "delayed"}' | jq .

echo "Triggering commit..."
curl -s -X POST "$API_URL/index_docs" -H "Content-Type: application/json" -d '{
  "commit": true,
  "docs": []
}'

echo "Searching for 'delayed' again (should match)..."
curl -s -X POST "$API_URL/search" -H "Content-Type: application/json" -d '{"q": "delayed"}' | jq .
