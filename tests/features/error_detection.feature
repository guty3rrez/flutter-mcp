# language: en
Feature: Passive uncaught exception detection
  As an AI coding agent
  I want unhandled exceptions detected from stdout/stderr without the app needing custom
  error-reporting code
  In order to know a hot reload broke something without having to inspect a screenshot

  Scenario: Detecting a Flutter red screen exception block
    Given a Flutter app printed a red screen exception block for "Null check operator used on a null value"
    When the error detector processes the captured output line by line
    Then it should report exactly 1 detected error
    And the detected error message should mention "EXCEPTION CAUGHT BY"
    And the detected error stack trace should mention "Null check operator"

  Scenario: Detecting a plain unhandled Dart exception without a red screen
    Given a Dart app printed an unhandled exception with message "Exception: algo salió mal"
    When the error detector processes the captured output line by line
    Then it should report exactly 1 detected error
    And the detected error message should mention "Unhandled exception"
