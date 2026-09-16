# language: en
Feature: UI Tree Pruning and Snapshot Generation
  As an AI coding agent
  I want to receive a clean, noise-free Flutter widget snapshot
  In order to interact accurately without wasting prompt tokens on layout wrappers

  Scenario: Pruning redundant Padding wrappers around an interactive button
    Given a Flutter diagnostics tree containing a Padding wrapper around an ElevatedButton with key "btn_login" and text "Iniciar Sesion"
    When the tree pruner processes the raw diagnostics tree
    Then the resulting node should have widget_type "ElevatedButton"
    And the node should have key "btn_login"
    And the node should have text "Iniciar Sesion"
    And the node should be marked as interactive
